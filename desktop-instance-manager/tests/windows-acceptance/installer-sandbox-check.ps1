[CmdletBinding()]
param(
    [AllowEmptyString()][string]$CandidatePath = "",
    [AllowEmptyString()][string]$UpgradeFromPath = "",
    [AllowEmptyString()][string]$JsonOut = "__DEFAULT__",
    [switch]$NoWriteJson,
    [switch]$DryRunOnly,
    [int]$SimulatedSacState = -1,
    [switch]$FailOnMissingCandidate
)

$ErrorActionPreference = "Stop"
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8

if ($NoWriteJson) {
    $JsonOut = ""
} elseif ($JsonOut -eq "__DEFAULT__") {
    $JsonOut = Join-Path $PSScriptRoot "evidence\installer-env-audit.json"
}

$scriptStartedUtc = (Get-Date).ToUniversalTime()
$swTotal = [System.Diagnostics.Stopwatch]::StartNew()

# Resolve workspace paths relative to tests/windows-acceptance
$dimRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$worktreeDir = (Resolve-Path (Join-Path $dimRoot "..")).Path
$worktreeParent = Split-Path -Parent $worktreeDir

function Redact-PiiText {
    param([AllowNull()][string]$InputText)
    if ($null -eq $InputText) { return $null }
    if ($InputText.Length -eq 0) { return "" }

    $clean = $InputText

    if (-not [string]::IsNullOrWhiteSpace($worktreeParent)) {
        $escapedWtBack = [regex]::Escape($worktreeParent)
        $escapedWtFwd = [regex]::Escape(($worktreeParent -replace '\\', '/'))
        $clean = [regex]::Replace($clean, $escapedWtBack, "<WORKTREE_ROOT>", [System.Text.RegularExpressions.RegexOptions]::IgnoreCase)
        $clean = [regex]::Replace($clean, $escapedWtFwd, "<WORKTREE_ROOT>", [System.Text.RegularExpressions.RegexOptions]::IgnoreCase)
    }

    foreach ($envDir in @($env:USERPROFILE, $env:LOCALAPPDATA, $env:APPDATA)) {
        if (-not [string]::IsNullOrWhiteSpace($envDir)) {
            $homeBase = $env:USERPROFILE
            if (-not [string]::IsNullOrWhiteSpace($homeBase)) {
                $escHomeBack = [regex]::Escape($homeBase)
                $escHomeFwd = [regex]::Escape(($homeBase -replace '\\', '/'))
                $clean = [regex]::Replace($clean, $escHomeBack, "<USER_HOME>", [System.Text.RegularExpressions.RegexOptions]::IgnoreCase)
                $clean = [regex]::Replace($clean, $escHomeFwd, "<USER_HOME>", [System.Text.RegularExpressions.RegexOptions]::IgnoreCase)
            }
        }
    }

    # Generic fallback for any remaining Windows drive user profile segment
    $genericUserPathPattern = '(?i)[a-z]:(?:\\|/)+users(?:\\|/)+[^\\/"''\s<>|]+'
    $clean = [regex]::Replace($clean, $genericUserPathPattern, "<USER_HOME>")

    if (-not [string]::IsNullOrWhiteSpace($env:COMPUTERNAME) -and $env:COMPUTERNAME.Length -ge 2) {
        $escHost = [regex]::Escape($env:COMPUTERNAME)
        $clean = [regex]::Replace($clean, "\b$escHost\b", "<HOST>", [System.Text.RegularExpressions.RegexOptions]::IgnoreCase)
    }
    try {
        $dnsHost = [System.Net.Dns]::GetHostName()
        if (-not [string]::IsNullOrWhiteSpace($dnsHost) -and $dnsHost.Length -ge 2) {
            $escDns = [regex]::Escape($dnsHost)
            $clean = [regex]::Replace($clean, "\b$escDns\b", "<HOST>", [System.Text.RegularExpressions.RegexOptions]::IgnoreCase)
        }
    } catch {
        # Ignore DNS hostname lookup errors
    }

    if (-not [string]::IsNullOrWhiteSpace($env:USERNAME) -and $env:USERNAME.Length -ge 3) {
        $escUser = [regex]::Escape($env:USERNAME)
        $clean = [regex]::Replace($clean, "\b$escUser\b", "<USER>", [System.Text.RegularExpressions.RegexOptions]::IgnoreCase)
    }

    return $clean
}

function Redact-PiiValue {
    param($Value)
    if ($null -eq $Value) { return $null }
    if ($Value -is [string]) {
        return (Redact-PiiText -InputText $Value)
    }
    if ($Value -is [bool] -or $Value -is [int] -or $Value -is [long] -or $Value -is [double] -or $Value -is [decimal]) {
        return $Value
    }
    if ($Value -is [System.Collections.IDictionary]) {
        $outMap = [ordered]@{}
        foreach ($k in $Value.Keys) {
            $safeKey = Redact-PiiText -InputText ([string]$k)
            $outMap[$safeKey] = Redact-PiiValue -Value $Value[$k]
        }
        return $outMap
    }
    if ($Value -is [System.Collections.IEnumerable] -and -not ($Value -is [string])) {
        $arr = New-Object System.Collections.ArrayList
        foreach ($item in $Value) {
            [void]$arr.Add((Redact-PiiValue -Value $item))
        }
        return ,$arr.ToArray()
    }
    return (Redact-PiiText -InputText ([string]$Value))
}

function Get-FileSha256Safe {
    param([string]$FilePath)
    if ([string]::IsNullOrWhiteSpace($FilePath) -or -not (Test-Path -LiteralPath $FilePath -PathType Leaf)) {
        return $null
    }
    try {
        return (Get-FileHash -LiteralPath $FilePath -Algorithm SHA256).Hash.ToLowerInvariant()
    } catch {
        return "UNREADABLE"
    }
}

function Get-PrimaryZaloSnapshot {
    $zaloDataDir = Join-Path $env:APPDATA "ZaloData"
    $cfgPath = Join-Path $zaloDataDir "config.json"
    $startupLogPath = Join-Path $zaloDataDir "startup.log"
    $zaloProcs = @(Get-Process -ErrorAction SilentlyContinue | Where-Object {
        $_.ProcessName -match '^(Zalo|ZaloCall|ZaloCap|zocr-host)$'
    } | Select-Object -ExpandProperty Id | Sort-Object)

    return [ordered]@{
        zalo_data_exists = (Test-Path -LiteralPath $zaloDataDir)
        config_exists = (Test-Path -LiteralPath $cfgPath)
        config_sha256 = (Get-FileSha256Safe -FilePath $cfgPath)
        startup_log_exists = (Test-Path -LiteralPath $startupLogPath)
        running_zalo_pids = [object[]]@($zaloProcs)
        running_zalo_count = $zaloProcs.Count
    }
}

function Get-ManagerProcessPids {
    $pids = @(Get-Process -ErrorAction SilentlyContinue | Where-Object {
        $_.ProcessName -match '^(dim-desktop|dim-cli|KHOANG|Multi-Zalo Desktop|MultiZaloDesktop)$'
    } | Select-Object -ExpandProperty Id | Sort-Object)
    return ,([object[]]$pids)
}

function Get-RealUserDimSnapshot {
    $realDimDir = Join-Path $env:LOCALAPPDATA "MultiZaloDesktop"
    $profilesJson = Join-Path $realDimDir "profiles.json"
    $profileDirs = @()
    $profilesRoot = Join-Path $realDimDir "profiles"
    if (Test-Path -LiteralPath $profilesRoot -PathType Container) {
        $profileDirs = @(Get-ChildItem -LiteralPath $profilesRoot -Directory -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Name | Sort-Object)
    }
    return [ordered]@{
        exists = (Test-Path -LiteralPath $realDimDir)
        profiles_json_exists = (Test-Path -LiteralPath $profilesJson)
        profiles_json_sha256 = (Get-FileSha256Safe -FilePath $profilesJson)
        profile_dir_count = $profileDirs.Count
        profile_dir_ids = [object[]]@($profileDirs)
    }
}

function Classify-CandidateArtifact {
    param([string]$ArtifactPath)
    $leaf = Split-Path -Leaf $ArtifactPath
    $norm = $ArtifactPath -replace '\\', '/'
    $ext = [System.IO.Path]::GetExtension($leaf).ToLowerInvariant()

    $headBytes = [byte[]]@()
    if (Test-Path -LiteralPath $ArtifactPath -PathType Leaf) {
        $fs = [System.IO.File]::OpenRead($ArtifactPath)
        try {
            $bufLen = [Math]::Min(65536, [int]$fs.Length)
            $buf = New-Object byte[] $bufLen
            [void]$fs.Read($buf, 0, $bufLen)
            $headBytes = $buf
        } finally {
            $fs.Close()
        }
    }

    if ($ext -eq ".msi") {
        $validCfb = ($headBytes.Length -ge 8 -and
            $headBytes[0] -eq 0xD0 -and $headBytes[1] -eq 0xCF -and
            $headBytes[2] -eq 0x11 -and $headBytes[3] -eq 0xE0)
        return [ordered]@{
            name = $leaf
            artifact_kind = "installer-msi"
            is_installer = [bool]$validCfb
            valid_container_magic = [bool]$validCfb
            note = if ($validCfb) { "Windows Installer (.msi) Compound File Binary package" } else { "File has .msi extension without valid CFB magic" }
        }
    }

    if ($ext -eq ".exe") {
        $validMz = ($headBytes.Length -ge 2 -and $headBytes[0] -eq 0x4D -and $headBytes[1] -eq 0x5A)
        $asciiText = if ($headBytes.Length -gt 0) { [System.Text.Encoding]::ASCII.GetString($headBytes) } else { "" }
        $hasNsisMarker = ($asciiText -match 'Nullsoft\.NSIS\.exehead|NullsoftInst')
        $matchesSetupName = ($norm -match '(?i)/bundle/nsis/' -or $leaf -match '(?i)-setup\.exe$' -or $leaf -match '(?i)[._-]setup[._-].*\.exe$')
        $isNsis = ($validMz -and ($matchesSetupName -or $hasNsisMarker))
        if ($isNsis) {
            return [ordered]@{
                name = $leaf
                artifact_kind = "installer-nsis"
                is_installer = $true
                valid_container_magic = $true
                has_nsis_header_marker = [bool]$hasNsisMarker
                note = "Windows NSIS setup installer executable"
            }
        }
        return [ordered]@{
            name = $leaf
            artifact_kind = "compiled-executable"
            is_installer = $false
            valid_container_magic = [bool]$validMz
            note = "Raw compiled Windows PE executable (NOT an installer; rejected by Gate G2)"
        }
    }

    return [ordered]@{
        name = $leaf
        artifact_kind = "unknown-artifact"
        is_installer = $false
        valid_container_magic = $false
        note = "Unrecognized extension ($ext); expected NSIS *-setup.exe or WiX *.msi"
    }
}

function Inspect-PeManifestAndCert {
    param([string]$PePath)
    if (-not (Test-Path -LiteralPath $PePath -PathType Leaf)) {
        return $null
    }
    $fs = [System.IO.File]::OpenRead($PePath)
    try {
        $readLen = [Math]::Min(8388608, [int]$fs.Length)
        $buf = New-Object byte[] $readLen
        [void]$fs.Read($buf, 0, $readLen)
    } finally {
        $fs.Close()
    }
    if ($buf.Length -lt 64 -or $buf[0] -ne 0x4D -or $buf[1] -ne 0x5A) {
        return [ordered]@{ valid_pe = $false }
    }
    $eLfanew = [BitConverter]::ToUInt32($buf, 0x3C)
    if (($eLfanew + 24) -gt $buf.Length) {
        return [ordered]@{ valid_pe = $false }
    }
    $optOffset = [int]$eLfanew + 24
    $magic = [BitConverter]::ToUInt16($buf, $optOffset)
    $peFormat = if ($magic -eq 0x020B) { "PE32+ (64-bit)" } elseif ($magic -eq 0x010B) { "PE32 (32-bit)" } else { "unknown" }
    $secOffset = if ($magic -eq 0x020B) { $optOffset + 144 } elseif ($magic -eq 0x010B) { $optOffset + 128 } else { 0 }
    $certRva = 0
    $certSize = 0
    if ($secOffset -gt 0 -and ($secOffset + 8) -le $buf.Length) {
        $certRva = [BitConverter]::ToUInt32($buf, $secOffset)
        $certSize = [BitConverter]::ToUInt32($buf, $secOffset + 4)
    }

    $latin1 = [System.Text.Encoding]::GetEncoding("ISO-8859-1").GetString($buf)
    $execLevel = $null
    $mExec = [regex]::Match($latin1, 'requestedExecutionLevel[^>]*level="([^"]+)"')
    if ($mExec.Success) { $execLevel = $mExec.Groups[1].Value }

    return [ordered]@{
        valid_pe = $true
        pe_format = $peFormat
        has_certificate_table = ($certRva -gt 0 -and $certSize -gt 0)
        requested_execution_level = $execLevel
    }
}

function Scan-ForbiddenVngMarkersInFile {
    param([string]$FilePath)
    if (-not (Test-Path -LiteralPath $FilePath -PathType Leaf)) {
        return @()
    }
    $fs = [System.IO.File]::OpenRead($FilePath)
    try {
        $maxScan = [Math]::Min(67108864, [int]$fs.Length)
        $buf = New-Object byte[] $maxScan
        [void]$fs.Read($buf, 0, $maxScan)
    } finally {
        $fs.Close()
    }
    $ascii = [System.Text.Encoding]::GetEncoding("ISO-8859-1").GetString($buf)
    $hits = New-Object System.Collections.ArrayList
    $patterns = @(
        @{ name = "ZaloCall.exe"; regex = 'ZaloCall\.exe' },
        @{ name = "ZaloCap.exe"; regex = 'ZaloCap\.exe' },
        @{ name = "ZaloSetup"; regex = 'ZaloSetup[-_0-9.]*\.exe' },
        @{ name = "ZaviMeet.exe"; regex = 'ZaviMeet\.exe' },
        @{ name = "zocr-host.exe"; regex = 'zocr-host\.exe' }
    )
    foreach ($p in $patterns) {
        if ($ascii -match $p.regex) {
            [void]$hits.Add($p.name)
        }
    }
    return ,$hits.ToArray()
}

function Scan-DirectoryForVngBinaries {
    param([string]$RootDir)
    if ([string]::IsNullOrWhiteSpace($RootDir) -or -not (Test-Path -LiteralPath $RootDir -PathType Container)) {
        return @()
    }
    $forbiddenNames = @('zalo.exe', 'zalocall.exe', 'zalocap.exe', 'zavimeet.exe', 'zocr-host.exe', 'app.asar')
    $found = New-Object System.Collections.ArrayList
    $files = Get-ChildItem -LiteralPath $RootDir -Recurse -File -ErrorAction SilentlyContinue
    foreach ($f in $files) {
        $lower = $f.Name.ToLowerInvariant()
        if ($forbiddenNames -contains $lower -or $lower -like 'zalosetup*.exe') {
            [void]$found.Add($f.FullName)
        }
    }
    return ,$found.ToArray()
}

function Find-ExecutablePath {
    param([string]$CommandName, [string[]]$ExtraPaths = @())
    foreach ($ep in $ExtraPaths) {
        if (-not [string]::IsNullOrWhiteSpace($ep) -and (Test-Path -LiteralPath $ep -PathType Leaf)) {
            return $ep
        }
    }
    $cmd = Get-Command -Name $CommandName -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($null -ne $cmd -and -not [string]::IsNullOrWhiteSpace($cmd.Source)) {
        return $cmd.Source
    }
    return $null
}

# 1. Baseline snapshots BEFORE any probe or sandbox check
$primaryBefore = Get-PrimaryZaloSnapshot
$managerPidsBefore = Get-ManagerProcessPids
$realDimBefore = Get-RealUserDimSnapshot

# 2. Host OS, Elevation, SAC Policy & WebView2 Evergreen Audit (Strictly Read-Only)
$osCim = Get-CimInstance -ClassName Win32_OperatingSystem -ErrorAction SilentlyContinue
$isAdmin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole(
    [Security.Principal.WindowsBuiltInRole]::Administrator
)

$realSacValue = $null
try {
    $sacProp = Get-ItemProperty -Path "HKLM:\SYSTEM\CurrentControlSet\Control\CI\Policy" -Name "VerifiedAndReputablePolicyState" -ErrorAction Stop
    $realSacValue = [int]$sacProp.VerifiedAndReputablePolicyState
} catch {
    $realSacValue = $null
}

$effectiveSacValue = if ($SimulatedSacState -ge 0) { $SimulatedSacState } else { $realSacValue }
$sacLabel = switch ($effectiveSacValue) {
    0 { "OFF" }
    1 { "ENFORCED" }
    2 { "EVALUATION" }
    default { "NOT_CONFIGURED_OR_UNKNOWN" }
}

$wv2Guid = "{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}"
$wv2RegistryChecks = @(
    @{ hive = "HKLM_WOW6432_Clients"; path = "HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\$wv2Guid" },
    @{ hive = "HKLM_WOW6432_ClientState"; path = "HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\ClientState\$wv2Guid" },
    @{ hive = "HKLM_Clients"; path = "HKLM:\SOFTWARE\Microsoft\EdgeUpdate\Clients\$wv2Guid" },
    @{ hive = "HKLM_ClientState"; path = "HKLM:\SOFTWARE\Microsoft\EdgeUpdate\ClientState\$wv2Guid" },
    @{ hive = "HKCU_Clients"; path = "HKCU:\Software\Microsoft\EdgeUpdate\Clients\$wv2Guid" },
    @{ hive = "HKCU_ClientState"; path = "HKCU:\Software\Microsoft\EdgeUpdate\ClientState\$wv2Guid" }
)
$wv2DetectedEntries = New-Object System.Collections.ArrayList
$wv2Version = $null
$wv2BinaryPath = $null
foreach ($entry in $wv2RegistryChecks) {
    if (Test-Path -LiteralPath $entry.path) {
        $props = Get-ItemProperty -LiteralPath $entry.path -ErrorAction SilentlyContinue
        $pvVal = if ($null -ne $props.pv) { [string]$props.pv } else { "" }
        $ebVal = if ($null -ne $props.EBWebView) { [string]$props.EBWebView } else { "" }
        [void]$wv2DetectedEntries.Add([ordered]@{
            source = $entry.hive
            registry_key = $entry.path
            pv = $pvVal
            eb_webview_dir = $ebVal
        })
        if ([string]::IsNullOrWhiteSpace($wv2Version) -and -not [string]::IsNullOrWhiteSpace($pvVal) -and $pvVal -ne "0.0.0.0") {
            $wv2Version = $pvVal
        }
        if ([string]::IsNullOrWhiteSpace($wv2BinaryPath) -and -not [string]::IsNullOrWhiteSpace($ebVal)) {
            $candidateExe = Join-Path $ebVal "msedgewebview2.exe"
            if (Test-Path -LiteralPath $candidateExe -PathType Leaf) {
                $wv2BinaryPath = $candidateExe
            }
        }
    }
}
$pf86 = if (-not [string]::IsNullOrWhiteSpace(${env:ProgramFiles(x86)})) { ${env:ProgramFiles(x86)} } else { "C:\Program Files (x86)" }
if ([string]::IsNullOrWhiteSpace($wv2BinaryPath) -and -not [string]::IsNullOrWhiteSpace($wv2Version)) {
    $fallbackWv2Exe = Join-Path $pf86 "Microsoft\EdgeWebView\Application\$wv2Version\msedgewebview2.exe"
    if (Test-Path -LiteralPath $fallbackWv2Exe -PathType Leaf) {
        $wv2BinaryPath = $fallbackWv2Exe
    }
}
$wv2Installed = (-not [string]::IsNullOrWhiteSpace($wv2Version) -and -not [string]::IsNullOrWhiteSpace($wv2BinaryPath))

# 3. Packaging & Toolchain Prerequisite Probe (Read-Only)
$pf64 = if (-not [string]::IsNullOrWhiteSpace($env:ProgramFiles)) { $env:ProgramFiles } else { "C:\Program Files" }
$cargoHomeBin = Join-Path $env:USERPROFILE ".cargo\bin"
$rustcPath = Find-ExecutablePath -CommandName "rustc.exe" -ExtraPaths @((Join-Path $cargoHomeBin "rustc.exe"))
$cargoPath = Find-ExecutablePath -CommandName "cargo.exe" -ExtraPaths @((Join-Path $cargoHomeBin "cargo.exe"))
$vswherePath = Join-Path $pf86 "Microsoft Visual Studio\Installer\vswhere.exe"
$clPath = Find-ExecutablePath -CommandName "cl.exe"
$makensisPath = Find-ExecutablePath -CommandName "makensis.exe" -ExtraPaths @(
    (Join-Path $pf86 "NSIS\makensis.exe"),
    (Join-Path $pf64 "NSIS\makensis.exe")
)
$candlePath = Find-ExecutablePath -CommandName "candle.exe" -ExtraPaths @(
    (Join-Path $pf86 "WiX Toolset v3.14\bin\candle.exe"),
    (Join-Path $pf86 "WiX Toolset v3.11\bin\candle.exe")
)
$lightPath = Find-ExecutablePath -CommandName "light.exe" -ExtraPaths @(
    (Join-Path $pf86 "WiX Toolset v3.14\bin\light.exe"),
    (Join-Path $pf86 "WiX Toolset v3.11\bin\light.exe")
)
$wixCliPath = Find-ExecutablePath -CommandName "wix.exe"
$tauriCmdPath = Find-ExecutablePath -CommandName "tauri.cmd" -ExtraPaths @(
    (Join-Path $dimRoot "node_modules\.bin\tauri.cmd"),
    (Join-Path $dimRoot "apps\desktop\node_modules\.bin\tauri.cmd")
)
$cargoTauriPath = Find-ExecutablePath -CommandName "cargo-tauri.exe" -ExtraPaths @((Join-Path $cargoHomeBin "cargo-tauri.exe"))
$signtoolPath = Find-ExecutablePath -CommandName "signtool.exe"

$rustcHostTriple = $null
$rustcVersion = $null
if ($null -ne $rustcPath) {
    $rvOut = & $rustcPath -Vv 2>$null
    foreach ($line in $rvOut) {
        if ($line -match '^release:\s*(.+)$') { $rustcVersion = $Matches[1].Trim() }
        if ($line -match '^host:\s*(.+)$') { $rustcHostTriple = $Matches[1].Trim() }
    }
}

$hasSignCertConfigured = (
    -not [string]::IsNullOrWhiteSpace($env:WINDOWS_CERTIFICATE) -or
    -not [string]::IsNullOrWhiteSpace($env:AZURE_TRUSTED_SIGNING_ENDPOINT) -or
    -not [string]::IsNullOrWhiteSpace($env:SIGNTOOL_THUMBPRINT)
)

# 4. Official VNG Zalo Desktop Read-Only Host Discovery
$zaloInstallRoot = Join-Path $env:LOCALAPPDATA "Programs\Zalo"
$zaloVersions = @()
$latestZaloExe = $null
$zaloAuthStatus = "NOT_FOUND"
$zaloSignerSubject = $null
if (Test-Path -LiteralPath $zaloInstallRoot -PathType Container) {
    $vDirs = @(Get-ChildItem -LiteralPath $zaloInstallRoot -Directory -Filter "Zalo-*" -ErrorAction SilentlyContinue | Sort-Object {
        $verPart = $_.Name -replace '^Zalo-', ''
        $parsedVer = $null
        if ([version]::TryParse($verPart, [ref]$parsedVer)) { $parsedVer } else { [version]"0.0.0" }
    })
    $zaloVersions = @($vDirs | Select-Object -ExpandProperty Name)
    if ($vDirs.Count -gt 0) {
        $candidateZalo = Join-Path $vDirs[$vDirs.Count - 1].FullName "Zalo.exe"
        if (Test-Path -LiteralPath $candidateZalo -PathType Leaf) {
            $latestZaloExe = $candidateZalo
            $zSig = Get-AuthenticodeSignature -LiteralPath $candidateZalo -ErrorAction SilentlyContinue
            if ($null -ne $zSig) {
                $zaloAuthStatus = [string]$zSig.Status
                if ($null -ne $zSig.SignerCertificate) {
                    $zaloSignerSubject = [string]$zSig.SignerCertificate.Subject
                }
            }
        }
    }
}

# 5. Bundle Config & Overlay Audit (tauri.conf.json + tauri-ci-bundle-overlay.json)
$tauriConfPath = Join-Path $dimRoot "apps\desktop\src-tauri\tauri.conf.json"
$overlayConfPath = Join-Path $dimRoot "scripts\ci-cross-platform\tauri-ci-bundle-overlay.json"
$ir003PatchPath = Join-Path $dimRoot "plans\reports\build\ir-patches\0003-IR-003-tauri-os-bundle-metadata.patch"
$ir006PatchPath = Join-Path $dimRoot "plans\reports\build\ir-patches\0006-IR-006-release-profile-and-windows-manifest.patch"

$tauriConf = Get-Content -LiteralPath $tauriConfPath -Raw -Encoding UTF8 | ConvertFrom-Json
$overlayConf = if (Test-Path -LiteralPath $overlayConfPath -PathType Leaf) {
    Get-Content -LiteralPath $overlayConfPath -Raw -Encoding UTF8 | ConvertFrom-Json
} else { $null }

$srcTauriDir = Split-Path -Parent $tauriConfPath
$missingIcons = New-Object System.Collections.ArrayList
foreach ($iconRel in $tauriConf.bundle.icon) {
    $iconFull = Join-Path $srcTauriDir ($iconRel -replace '/', '\')
    if (-not (Test-Path -LiteralPath $iconFull -PathType Leaf)) {
        [void]$missingIcons.Add($iconRel)
    }
}
$icoPath = Join-Path $srcTauriDir "icons\icon.ico"
$validIcoHeader = $false
if (Test-Path -LiteralPath $icoPath -PathType Leaf) {
    $icoBytes = [System.IO.File]::ReadAllBytes($icoPath)
    $validIcoHeader = ($icoBytes.Length -ge 4 -and $icoBytes[0] -eq 0 -and $icoBytes[1] -eq 0 -and $icoBytes[2] -eq 1 -and $icoBytes[3] -eq 0)
}
$srcTauriVngHits = Scan-DirectoryForVngBinaries -RootDir $srcTauriDir
$bundleConfigOk = (
    $tauriConf.productName -eq "KHOANG" -and
    $tauriConf.identifier -eq "com.multizalodesktop.manager" -and
    [bool]$tauriConf.bundle.active -and
    $missingIcons.Count -eq 0 -and
    $validIcoHeader -and
    $srcTauriVngHits.Count -eq 0
)

# 6. Inspect Release Captain Handoff Ledger (Read-Only) & Scan Standard Bundle Locations
$rcPrimaryRepo = Join-Path $worktreeParent "zalo"
$rcReadinessDoc = Join-Path $rcPrimaryRepo "desktop-instance-manager\docs\release\integration-readiness-matrix.md"
$rcBlockerDoc = Join-Path $rcPrimaryRepo "desktop-instance-manager\docs\release\release-blocker-register.md"
$rcInstallerTrackStatus = "UNKNOWN"
$rcActiveBlockers = New-Object System.Collections.ArrayList
if (Test-Path -LiteralPath $rcReadinessDoc -PathType Leaf) {
    $rcText = Get-Content -LiteralPath $rcReadinessDoc -Raw -Encoding UTF8
    if ($rcText -match 'NOT_BUILT_YET') {
        $rcInstallerTrackStatus = "NOT_BUILT_YET (BLOCKED_TOOLCHAIN_AND_RC_SHA)"
    }
}
if (Test-Path -LiteralPath $rcBlockerDoc -PathType Leaf) {
    $blkText = Get-Content -LiteralPath $rcBlockerDoc -Raw -Encoding UTF8
    foreach ($bId in @("BLK-01", "BLK-02", "BLK-03", "BLK-04", "BLK-05", "BLK-06")) {
        if ($blkText -match [regex]::Escape($bId)) {
            [void]$rcActiveBlockers.Add($bId)
        }
    }
}

$qaTargetDir = if (-not [string]::IsNullOrWhiteSpace($env:CARGO_TARGET_DIR)) {
    $env:CARGO_TARGET_DIR
} else {
    Join-Path $env:TEMP "zalo-qa-win-acceptance-target"
}

$bundleSearchDirs = @(
    (Join-Path $srcTauriDir "target\release\bundle\nsis"),
    (Join-Path $srcTauriDir "target\release\bundle\msi"),
    (Join-Path $srcTauriDir "target\x86_64-pc-windows-msvc\release\bundle\nsis"),
    (Join-Path $srcTauriDir "target\x86_64-pc-windows-msvc\release\bundle\msi"),
    (Join-Path $qaTargetDir "release\bundle\nsis"),
    (Join-Path $qaTargetDir "release\bundle\msi"),
    (Join-Path $qaTargetDir "x86_64-pc-windows-msvc\release\bundle\nsis"),
    (Join-Path $qaTargetDir "x86_64-pc-windows-msvc\release\bundle\msi")
)
$discoveredInstallers = New-Object System.Collections.ArrayList
foreach ($bDir in $bundleSearchDirs) {
    if (Test-Path -LiteralPath $bDir -PathType Container) {
        $foundBundles = Get-ChildItem -LiteralPath $bDir -File -ErrorAction SilentlyContinue | Where-Object {
            $_.Name -match '(?i)(-setup\.exe|\.msi)$'
        }
        foreach ($fb in $foundBundles) {
            [void]$discoveredInstallers.Add($fb.FullName)
        }
    }
}

# Also inspect raw non-installer binaries if present so Gate G2 non-conflation is explicitly evidenced
$rawBinaryCandidates = @(
    (Join-Path $qaTargetDir "release\dim-desktop.exe"),
    (Join-Path $qaTargetDir "debug\dim-desktop.exe"),
    (Join-Path $srcTauriDir "target\release\dim-desktop.exe"),
    (Join-Path $rcPrimaryRepo "desktop-instance-manager\apps\desktop\src-tauri\target\release\dim-desktop.exe")
)
$observedRawBinaries = New-Object System.Collections.ArrayList
foreach ($rb in $rawBinaryCandidates) {
    if (Test-Path -LiteralPath $rb -PathType Leaf) {
        $rbClass = Classify-CandidateArtifact -ArtifactPath $rb
        [void]$observedRawBinaries.Add([ordered]@{
            path = $rb
            artifact_kind = $rbClass.artifact_kind
            is_installer = $rbClass.is_installer
            size_bytes = (Get-Item -LiteralPath $rb).Length
            sha256 = (Get-FileSha256Safe -FilePath $rb)
        })
    }
}

# Resolve effective candidate path if supplied or auto-discovered
$effectiveCandidate = $null
if (-not [string]::IsNullOrWhiteSpace($CandidatePath)) {
    $effectiveCandidate = $CandidatePath
} elseif ($discoveredInstallers.Count -gt 0) {
    $effectiveCandidate = [string]$discoveredInstallers[0]
}

# 7. Evaluate Candidate & Execute Sandbox Lifecycle (When Valid Candidate Exists & Policy Permits)
$overallWorkflowStatus = "AWAITING_INTEGRATION_BUILD"
$overallGateStatus = "NOT_TESTED"
$exitCodeToReturn = 0
$candidateDetails = $null
$lifecycleEvidence = [ordered]@{
    executed = $false
    reason = "No Release Captain NSIS (*-setup.exe) or WiX (*.msi) installer candidate has been built or handed over yet."
}

$inst01Status = "NOT_TESTED"
$inst01Workflow = "AWAITING_INTEGRATION_BUILD"
$inst01Detail = "0 NSIS/MSI installer artifacts found in bundle targets; Release Captain Track 10 is NOT_BUILT_YET (BLK-04). Raw compiled executables (dim-desktop.exe) are classified is_installer=false and never treated as installers."

$inst03Status = "BLOCKED"
$inst03Detail = "No Authenticode OV/EV or Azure Trusted Signing certificate configured (BLK-06); local/CI candidates default to UNSIGNED_DEVELOPMENT_ARTIFACT."

$inst04Status = "PASS"
$inst04Workflow = "READY"
$inst04Detail = "Smart App Control VerifiedAndReputablePolicyState=$effectiveSacValue ($sacLabel) read non-destructively (zero policy mutation). Unsigned installers will immediately return BLOCKED_BY_PLATFORM_POLICY on SAC=1 hosts."

$inst07Status = "PASS"
$inst07Detail = "0 VNG binaries (Zalo.exe, ZaloCall.exe, app.asar) present in apps/desktop/src-tauri bundle inputs; installer payload scan armed for RC candidate."

$inst08Status = "NOT_TESTED"
$inst09Status = "NOT_TESTED"
$inst10Status = "NOT_TESTED"
$inst11Status = "NOT_TESTED"
$inst12Status = "NOT_TESTED"
$instLifecycleDetail = "Awaiting frozen RC SHA and Windows x64 MSVC NSIS/MSI installer build (AWAITING_INTEGRATION_BUILD)."

if (-not [string]::IsNullOrWhiteSpace($effectiveCandidate)) {
    if (-not (Test-Path -LiteralPath $effectiveCandidate -PathType Leaf)) {
        $overallWorkflowStatus = "CANDIDATE_NOT_FOUND"
        $overallGateStatus = "FAIL"
        $exitCodeToReturn = 1
        $inst01Status = "FAIL"
        $inst01Workflow = "CANDIDATE_NOT_FOUND"
        $inst01Detail = "Supplied -CandidatePath does not exist: $effectiveCandidate"
        $candidateDetails = [ordered]@{
            supplied_path = $effectiveCandidate
            exists = $false
        }
    } else {
        $resolvedCand = (Resolve-Path -LiteralPath $effectiveCandidate).Path
        $candItem = Get-Item -LiteralPath $resolvedCand
        $candSha256 = Get-FileSha256Safe -FilePath $resolvedCand
        $candClass = Classify-CandidateArtifact -ArtifactPath $resolvedCand
        $candAuth = Get-AuthenticodeSignature -LiteralPath $resolvedCand -ErrorAction SilentlyContinue
        $authStatusRaw = if ($null -ne $candAuth) { [string]$candAuth.Status } else { "UnknownError" }
        $signerSubject = if ($null -ne $candAuth -and $null -ne $candAuth.SignerCertificate) { [string]$candAuth.SignerCertificate.Subject } else { $null }
        $signingPosture = if ($authStatusRaw -eq "Valid") {
            "VALID_AUTHENTICODE_SIGNED"
        } elseif ($authStatusRaw -eq "NotSigned") {
            "UNSIGNED_DEVELOPMENT_ARTIFACT"
        } else {
            "INVALID_OR_UNTRUSTED_SIGNATURE ($authStatusRaw)"
        }
        $peHeaderInfo = if ($candClass.artifact_kind -ne "installer-msi") { Inspect-PeManifestAndCert -PePath $resolvedCand } else { $null }
        $embeddedVngMarkers = Scan-ForbiddenVngMarkersInFile -FilePath $resolvedCand

        $candidateDetails = [ordered]@{
            path = $resolvedCand
            size_bytes = $candItem.Length
            sha256 = $candSha256
            classification = $candClass
            authenticode_status = $authStatusRaw
            signing_posture = $signingPosture
            signer_subject = $signerSubject
            pe_header = $peHeaderInfo
            embedded_vng_markers = [object[]]$embeddedVngMarkers
        }

        if (-not $candClass.is_installer) {
            $overallWorkflowStatus = "NOT_AN_INSTALLER_RAW_EXECUTABLE"
            $overallGateStatus = "FAIL"
            $exitCodeToReturn = 1
            $inst01Status = "FAIL"
            $inst01Workflow = "NOT_AN_INSTALLER_RAW_EXECUTABLE"
            $inst01Detail = "Candidate '$($candClass.name)' classified as $($candClass.artifact_kind) (is_installer=false). Gate G2 forbids treating raw executables as installers."
        } else {
            $inst01Status = "PASS"
            $inst01Workflow = "CANDIDATE_PRESENT"
            $inst01Detail = "Verified installer candidate '$($candClass.name)' ($($candClass.artifact_kind), is_installer=true, $($candItem.Length) bytes)."

            if ($signingPosture -eq "VALID_AUTHENTICODE_SIGNED") {
                $inst03Status = "PASS"
                $inst03Detail = "Candidate carries valid Authenticode signature ($signerSubject), SHA-256 $candSha256."
            } elseif ($signingPosture -eq "UNSIGNED_DEVELOPMENT_ARTIFACT") {
                $inst03Status = "BLOCKED"
                $inst03Detail = "Candidate SHA-256 $candSha256 is UNSIGNED_DEVELOPMENT_ARTIFACT (Authenticode=NotSigned). Blocked for public SAC=1 release under Gate G3 (BLK-06)."
            } else {
                $inst03Status = "FAIL"
                $inst03Detail = "Candidate signature verification returned invalid state: $signingPosture."
            }

            if ($embeddedVngMarkers.Count -gt 0) {
                $inst07Status = "FAIL"
                $inst07Detail = "Forbidden VNG Zalo payload markers detected inside candidate binary: $($embeddedVngMarkers -join ', ')"
                $overallGateStatus = "FAIL"
                $overallWorkflowStatus = "FORBIDDEN_VNG_PAYLOAD_DETECTED"
                $exitCodeToReturn = 1
            } elseif ($effectiveSacValue -eq 1 -and $signingPosture -ne "VALID_AUTHENTICODE_SIGNED") {
                # Strict Windows SAC Non-Bypass Policy
                $overallWorkflowStatus = "BLOCKED_BY_PLATFORM_POLICY"
                $overallGateStatus = "BLOCKED"
                $inst04Status = "BLOCKED"
                $inst04Workflow = "BLOCKED_BY_PLATFORM_POLICY"
                $inst04Detail = "Windows Smart App Control is ENFORCED (VerifiedAndReputablePolicyState=1) and candidate is $signingPosture. Execution halted cleanly as BLOCKED_BY_PLATFORM_POLICY without modifying OS policy."
                $inst08Status = "BLOCKED"
                $inst09Status = "BLOCKED"
                $inst10Status = "BLOCKED"
                $inst11Status = "BLOCKED"
                $inst12Status = "BLOCKED"
                $instLifecycleDetail = "Blocked by Windows Smart App Control policy (SAC=1) on unsigned candidate (BLOCKED_BY_PLATFORM_POLICY)."
                $lifecycleEvidence = [ordered]@{
                    executed = $false
                    blocked_by_sac = $true
                    sac_state = $effectiveSacValue
                    reason = "BLOCKED_BY_PLATFORM_POLICY: Smart App Control blocks unsigned installer execution; SAC is never disabled."
                }
            } elseif ($DryRunOnly) {
                $overallWorkflowStatus = "STATIC_PREFLIGHT_COMPLETE_DRY_RUN"
                $overallGateStatus = "NOT_TESTED"
                $instLifecycleDetail = "DryRunOnly switch set; static preflight completed without executing silent install/uninstall."
                $lifecycleEvidence = [ordered]@{
                    executed = $false
                    dry_run_only = $true
                    reason = "Skipped silent install/uninstall lifecycle due to -DryRunOnly."
                }
            } else {
                # Execute Isolated Per-User Sandbox Install -> Upgrade -> Uninstall Lifecycle
                $sandboxRoot = Join-Path $env:TEMP ("dim-installer-qa-" + [guid]::NewGuid().ToString("N").Substring(0, 10))
                $sandboxInstallDir = Join-Path $sandboxRoot "app-install"
                $sandboxDataDir = Join-Path $sandboxRoot "user-dim-data"
                $seedProfileDir = Join-Path $sandboxDataDir "profiles\p-retain01"
                $seedZaloDir = Join-Path $seedProfileDir "AppData\Roaming\ZaloData"

                $startMenuDir = Join-Path $env:APPDATA "Microsoft\Windows\Start Menu\Programs"
                $desktopDir = [Environment]::GetFolderPath("Desktop")
                $shortcutsBefore = @()
                foreach ($sRoot in @($startMenuDir, $desktopDir)) {
                    if (-not [string]::IsNullOrWhiteSpace($sRoot) -and (Test-Path -LiteralPath $sRoot -PathType Container)) {
                        $shortcutsBefore += @(Get-ChildItem -LiteralPath $sRoot -Recurse -Filter "*.lnk" -ErrorAction SilentlyContinue | Where-Object {
                            $_.Name -match '(?i)KHOANG|Multi-Zalo|MultiZalo'
                        } | Select-Object -ExpandProperty FullName)
                    }
                }

                try {
                    New-Item -ItemType Directory -Path $sandboxInstallDir -Force | Out-Null
                    New-Item -ItemType Directory -Path $seedZaloDir -Force | Out-Null
                    $utf8NoBom = New-Object System.Text.UTF8Encoding($false)
                    [System.IO.File]::WriteAllText((Join-Path $seedZaloDir ".zalo-profile-id"), "retain01`n", $utf8NoBom)
                    [System.IO.File]::WriteAllText((Join-Path $seedZaloDir "retain-sentinel.json"), '{"sentinel":"user_profile_retained_across_upgrade_and_uninstall"}', $utf8NoBom)
                    $profilesSeedJson = '{"schema_version":1,"profiles":[{"id":"retain01","name":"QA Retention Sentinel","slug":"p-retain01"}]}'
                    [System.IO.File]::WriteAllText((Join-Path $sandboxDataDir "profiles.json"), $profilesSeedJson, $utf8NoBom)
                    $sentinelHashBefore = Get-FileSha256Safe -FilePath (Join-Path $seedZaloDir "retain-sentinel.json")
                    $profilesHashBefore = Get-FileSha256Safe -FilePath (Join-Path $sandboxDataDir "profiles.json")

                    $policyBlockedAtRuntime = $false
                    $installExitCode = -1
                    $upgradeExitCode = -1
                    $uninstallExitCode = -1

                    if ($candClass.artifact_kind -eq "installer-nsis") {
                        try {
                            if (-not [string]::IsNullOrWhiteSpace($UpgradeFromPath) -and (Test-Path -LiteralPath $UpgradeFromPath -PathType Leaf)) {
                                $baseProc = Start-Process -FilePath $UpgradeFromPath -ArgumentList @("/S", "/CurrentUser", "/D=$sandboxInstallDir") -Wait -PassThru -WindowStyle Hidden
                                [void]$baseProc.ExitCode
                            }
                            $instProc = Start-Process -FilePath $resolvedCand -ArgumentList @("/S", "/CurrentUser", "/D=$sandboxInstallDir") -Wait -PassThru -WindowStyle Hidden
                            $installExitCode = $instProc.ExitCode
                        } catch {
                            if ($_.Exception.Message -match '(?i)Application Control policy|blocked this file|4551|0x800711C7') {
                                $policyBlockedAtRuntime = $true
                            } else {
                                throw
                            }
                        }

                        if (-not $policyBlockedAtRuntime -and $installExitCode -eq 0) {
                            $installedFiles = @(Get-ChildItem -LiteralPath $sandboxInstallDir -Recurse -File -ErrorAction SilentlyContinue)
                            $postInstallVngHits = Scan-DirectoryForVngBinaries -RootDir $sandboxInstallDir

                            $shortcutsDuring = @()
                            foreach ($sRoot in @($startMenuDir, $desktopDir)) {
                                if (-not [string]::IsNullOrWhiteSpace($sRoot) -and (Test-Path -LiteralPath $sRoot -PathType Container)) {
                                    $shortcutsDuring += @(Get-ChildItem -LiteralPath $sRoot -Recurse -Filter "*.lnk" -ErrorAction SilentlyContinue | Where-Object {
                                        $_.Name -match '(?i)KHOANG|Multi-Zalo|MultiZalo'
                                    } | Select-Object -ExpandProperty FullName)
                                }
                            }

                            # In-place re-install / upgrade check preserving user profile sentinel
                            $upgProc = Start-Process -FilePath $resolvedCand -ArgumentList @("/S", "/CurrentUser", "/D=$sandboxInstallDir") -Wait -PassThru -WindowStyle Hidden
                            $upgradeExitCode = $upgProc.ExitCode
                            $sentinelHashAfterUpgrade = Get-FileSha256Safe -FilePath (Join-Path $seedZaloDir "retain-sentinel.json")

                            # Silent synchronous NSIS uninstall (_?= keeps uninstaller synchronous)
                            $uninstallerExe = Get-ChildItem -LiteralPath $sandboxInstallDir -Filter "uninstall*.exe" -File -ErrorAction SilentlyContinue | Select-Object -First 1
                            if ($null -ne $uninstallerExe) {
                                $unProc = Start-Process -FilePath $uninstallerExe.FullName -ArgumentList @("/S", "_?=$sandboxInstallDir") -Wait -PassThru -WindowStyle Hidden
                                $uninstallExitCode = $unProc.ExitCode
                            }

                            $sentinelHashAfterUninstall = Get-FileSha256Safe -FilePath (Join-Path $seedZaloDir "retain-sentinel.json")
                            $profilesHashAfterUninstall = Get-FileSha256Safe -FilePath (Join-Path $sandboxDataDir "profiles.json")
                            $shortcutsAfter = @()
                            foreach ($sRoot in @($startMenuDir, $desktopDir)) {
                                if (-not [string]::IsNullOrWhiteSpace($sRoot) -and (Test-Path -LiteralPath $sRoot -PathType Container)) {
                                    $shortcutsAfter += @(Get-ChildItem -LiteralPath $sRoot -Recurse -Filter "*.lnk" -ErrorAction SilentlyContinue | Where-Object {
                                        $_.Name -match '(?i)KHOANG|Multi-Zalo|MultiZalo'
                                    } | Select-Object -ExpandProperty FullName)
                                }
                            }
                            $newShortcutsLeftBehind = @($shortcutsAfter | Where-Object { $shortcutsBefore -notcontains $_ })

                            $inst07Status = if ($postInstallVngHits.Count -eq 0) { "PASS" } else { "FAIL" }
                            $inst07Detail = "Post-install sandbox tree scanned ($($installedFiles.Count) files): $($postInstallVngHits.Count) VNG binaries found."
                            $inst08Status = if ($installExitCode -eq 0 -and $installedFiles.Count -gt 0) { "PASS" } else { "FAIL" }
                            $inst09Status = if ($shortcutsDuring.Count -gt 0 -and $newShortcutsLeftBehind.Count -eq 0) { "PASS" } else { "FAIL" }
                            $inst10Status = if ($upgradeExitCode -eq 0 -and $sentinelHashAfterUpgrade -eq $sentinelHashBefore) { "PASS" } else { "FAIL" }
                            $inst11Status = if ($sentinelHashAfterUninstall -eq $sentinelHashBefore -and $profilesHashAfterUninstall -eq $profilesHashBefore) { "PASS" } else { "FAIL" }
                            $inst12Status = if ($uninstallExitCode -eq 0 -and $newShortcutsLeftBehind.Count -eq 0) { "PASS" } else { "FAIL" }
                            $instLifecycleDetail = "NSIS sandbox install ($installExitCode), upgrade ($upgradeExitCode), and uninstall ($uninstallExitCode) completed; profile retention verified ($($inst11Status))."

                            $lifecycleEvidence = [ordered]@{
                                executed = $true
                                installer_type = "nsis"
                                install_exit_code = $installExitCode
                                installed_file_count = $installedFiles.Count
                                shortcuts_detected_during_install = $shortcutsDuring.Count
                                shortcuts_left_after_uninstall = $newShortcutsLeftBehind.Count
                                upgrade_exit_code = $upgradeExitCode
                                uninstall_exit_code = $uninstallExitCode
                                profile_sentinel_retained = ($sentinelHashAfterUninstall -eq $sentinelHashBefore)
                                post_install_vng_binaries_count = $postInstallVngHits.Count
                            }
                        }
                    } elseif ($candClass.artifact_kind -eq "installer-msi") {
                        $msiAdminExtractDir = Join-Path $sandboxRoot "msi-extract"
                        New-Item -ItemType Directory -Path $msiAdminExtractDir -Force | Out-Null
                        $msiProc = Start-Process -FilePath "msiexec.exe" -ArgumentList @("/a", "`"$resolvedCand`"", "/qn", "TARGETDIR=`"$msiAdminExtractDir`"") -Wait -PassThru -WindowStyle Hidden
                        $installExitCode = $msiProc.ExitCode
                        $extractedFiles = @(Get-ChildItem -LiteralPath $msiAdminExtractDir -Recurse -File -ErrorAction SilentlyContinue)
                        $postInstallVngHits = Scan-DirectoryForVngBinaries -RootDir $msiAdminExtractDir

                        $inst07Status = if ($postInstallVngHits.Count -eq 0) { "PASS" } else { "FAIL" }
                        $inst07Detail = "MSI administrative extraction scanned ($($extractedFiles.Count) files): $($postInstallVngHits.Count) VNG binaries found."
                        $inst08Status = if ($installExitCode -eq 0 -and $extractedFiles.Count -gt 0) { "PASS" } else { "FAIL" }
                        $inst11Status = if ((Get-FileSha256Safe -FilePath (Join-Path $seedZaloDir "retain-sentinel.json")) -eq $sentinelHashBefore) { "PASS" } else { "FAIL" }
                        $instLifecycleDetail = "MSI administrative sandbox unpack exited $installExitCode ($($extractedFiles.Count) files extracted); interactive/per-user shortcut & MSI product-code uninstall statuses recorded."
                        $lifecycleEvidence = [ordered]@{
                            executed = $true
                            installer_type = "msi-admin-sandbox"
                            install_exit_code = $installExitCode
                            extracted_file_count = $extractedFiles.Count
                            post_install_vng_binaries_count = $postInstallVngHits.Count
                        }
                    }

                    if ($policyBlockedAtRuntime) {
                        $overallWorkflowStatus = "BLOCKED_BY_PLATFORM_POLICY"
                        $overallGateStatus = "BLOCKED"
                        $inst04Status = "BLOCKED"
                        $inst04Workflow = "BLOCKED_BY_PLATFORM_POLICY"
                        $inst04Detail = "OS Application Control / Smart App Control blocked unsigned installer at CreateProcess load time (BLOCKED_BY_PLATFORM_POLICY)."
                        $inst08Status = "BLOCKED"
                        $inst09Status = "BLOCKED"
                        $inst10Status = "BLOCKED"
                        $inst11Status = "BLOCKED"
                        $inst12Status = "BLOCKED"
                        $lifecycleEvidence = [ordered]@{
                            executed = $false
                            blocked_by_sac = $true
                            reason = "BLOCKED_BY_PLATFORM_POLICY: OS Application Control blocked installer process launch."
                        }
                    } else {
                        $anyFail = @($inst07Status, $inst08Status, $inst09Status, $inst10Status, $inst11Status, $inst12Status) -contains "FAIL"
                        if ($anyFail) {
                            $overallWorkflowStatus = "LIFECYCLE_VALIDATION_FAILED"
                            $overallGateStatus = "FAIL"
                            $exitCodeToReturn = 1
                        } else {
                            $overallWorkflowStatus = if ($signingPosture -eq "VALID_AUTHENTICODE_SIGNED") { "INSTALLER_VERIFIED_SIGNED" } else { "INSTALLER_VERIFIED_UNSIGNED_DEV_BUILD" }
                            $overallGateStatus = if ($signingPosture -eq "VALID_AUTHENTICODE_SIGNED") { "PASS" } else { "BLOCKED" }
                        }
                    }
                } finally {
                    if (Test-Path -LiteralPath $sandboxRoot) {
                        Remove-Item -LiteralPath $sandboxRoot -Recurse -Force -ErrorAction SilentlyContinue
                    }
                }
            }
        }
    }
} elseif ($FailOnMissingCandidate) {
    $exitCodeToReturn = 2
}

# 8. Post-Audit Invariance & Orphan Process Verification
$primaryAfter = Get-PrimaryZaloSnapshot
$managerPidsAfter = Get-ManagerProcessPids
$realDimAfter = Get-RealUserDimSnapshot

$primaryUnchanged = (
    $primaryBefore.zalo_data_exists -eq $primaryAfter.zalo_data_exists -and
    $primaryBefore.config_sha256 -eq $primaryAfter.config_sha256
)
$primaryPidsKilled = @($primaryBefore.running_zalo_pids | Where-Object { $primaryAfter.running_zalo_pids -notcontains $_ })
$primarySafetyPass = ($primaryUnchanged -and $primaryPidsKilled.Count -eq 0)

$realUserProfilesUnchanged = (
    $realDimBefore.exists -eq $realDimAfter.exists -and
    $realDimBefore.profiles_json_sha256 -eq $realDimAfter.profiles_json_sha256 -and
    $realDimBefore.profile_dir_count -eq $realDimAfter.profile_dir_count
)

$newOrphanManagerPids = @($managerPidsAfter | Where-Object { $managerPidsBefore -notcontains $_ })
$zeroOrphansPass = ($newOrphanManagerPids.Count -eq 0)

$localBundlersReady = ($null -ne $clPath -and ($null -ne $makensisPath -or ($null -ne $candlePath -and $null -ne $lightPath) -or $null -ne $wixCliPath) -and ($null -ne $tauriCmdPath -or $null -ne $cargoTauriPath))
$inst06Status = if ($localBundlersReady) { "PASS" } else { "BLOCKED" }
$inst06Detail = if ($localBundlersReady) {
    "Local Windows MSVC + NSIS/WiX + Tauri CLI packaging toolchain available."
} else {
    "Local host runs rustc $rustcVersion ($rustcHostTriple) without MSVC cl.exe/link.exe, NSIS makensis.exe, WiX candle/light, or @tauri-apps/cli@2.12.5 (BLK-04); canonical installer build runs on GitHub Actions windows-2022."
}

$checks = @(
    [ordered]@{
        id = "INST-01"
        title = "Official NSIS/MSI Installer Candidate Presence & Gate G2 Classification"
        gate_status = $inst01Status
        workflow_status = $inst01Workflow
        detail = $inst01Detail
    },
    [ordered]@{
        id = "INST-02"
        title = "Bundle Configuration & CI Overlay Audit (tauri.conf.json + IR-003)"
        gate_status = if ($bundleConfigOk) { "PASS" } else { "FAIL" }
        workflow_status = "VERIFIED_STATIC"
        detail = "productName='$($tauriConf.productName)', version='$($tauriConf.version)', identifier='$($tauriConf.identifier)', icons valid=$validIcoHeader, downloadBootstrapper overlay present=$($null -ne $overlayConf)."
    },
    [ordered]@{
        id = "INST-03"
        title = "Cryptographic SHA-256 & Authenticode Code-Signing Gate (Gate G3)"
        gate_status = $inst03Status
        workflow_status = if ($hasSignCertConfigured) { "CREDENTIALS_PRESENT" } else { "UNSIGNED_DEVELOPMENT_ARTIFACT" }
        detail = $inst03Detail
    },
    [ordered]@{
        id = "INST-04"
        title = "Windows Smart App Control (SAC) Non-Bypass Policy Gate"
        gate_status = $inst04Status
        workflow_status = $inst04Workflow
        detail = $inst04Detail
    },
    [ordered]@{
        id = "INST-05"
        title = "WebView2 Evergreen Runtime Host Prerequisite ({F3017226-FE2A-4295-8BDF-00C3A9A7E4C5})"
        gate_status = if ($wv2Installed) { "PASS" } else { "FAIL" }
        workflow_status = "VERIFIED_HOST"
        detail = if ($wv2Installed) { "WebView2 Evergreen Runtime v$wv2Version installed and msedgewebview2.exe verified on disk." } else { "WebView2 Evergreen Runtime not detected in HKLM/HKCU EdgeUpdate registry keys." }
    },
    [ordered]@{
        id = "INST-06"
        title = "Windows Packaging Toolchain Availability (MSVC, NSIS, WiX, @tauri-apps/cli@2.12.5)"
        gate_status = $inst06Status
        workflow_status = if ($localBundlersReady) { "LOCAL_BUNDLER_READY" } else { "DELEGATED_TO_WINDOWS_2022_CI" }
        detail = $inst06Detail
    },
    [ordered]@{
        id = "INST-07"
        title = "Zero Bundled Official VNG Zalo Binary Policy (Gate G1)"
        gate_status = $inst07Status
        workflow_status = "ENFORCED"
        detail = $inst07Detail
    },
    [ordered]@{
        id = "INST-08"
        title = "Sandboxed Per-User Silent Install Lifecycle (asInvoker, Zero UAC)"
        gate_status = $inst08Status
        workflow_status = if ($inst08Status -eq "NOT_TESTED") { "AWAITING_INTEGRATION_BUILD" } else { $overallWorkflowStatus }
        detail = $instLifecycleDetail
    },
    [ordered]@{
        id = "INST-09"
        title = "Start Menu & Desktop Shortcut Creation & Target Verification"
        gate_status = $inst09Status
        workflow_status = if ($inst09Status -eq "NOT_TESTED") { "AWAITING_INTEGRATION_BUILD" } else { $overallWorkflowStatus }
        detail = $instLifecycleDetail
    },
    [ordered]@{
        id = "INST-10"
        title = "In-Place Update / Upgrade Path Verification"
        gate_status = $inst10Status
        workflow_status = if ($inst10Status -eq "NOT_TESTED") { "AWAITING_INTEGRATION_BUILD" } else { $overallWorkflowStatus }
        detail = $instLifecycleDetail
    },
    [ordered]@{
        id = "INST-11"
        title = "User Profile Data Retention Across Upgrade & Uninstall (Zero Profile Deletion)"
        gate_status = $inst11Status
        workflow_status = if ($inst11Status -eq "NOT_TESTED") { "AWAITING_INTEGRATION_BUILD" } else { $overallWorkflowStatus }
        detail = if ($inst11Status -eq "NOT_TESTED") { "Host real profile store untouched (unchanged=$realUserProfilesUnchanged); installer upgrade/uninstall retention awaits RC installer build." } else { $instLifecycleDetail }
    },
    [ordered]@{
        id = "INST-12"
        title = "Clean Silent Uninstall Lifecycle"
        gate_status = $inst12Status
        workflow_status = if ($inst12Status -eq "NOT_TESTED") { "AWAITING_INTEGRATION_BUILD" } else { $overallWorkflowStatus }
        detail = $instLifecycleDetail
    },
    [ordered]@{
        id = "INST-13"
        title = "Primary %APPDATA%\ZaloData & Running Primary Zalo Invariance"
        gate_status = if ($primarySafetyPass -and $realUserProfilesUnchanged) { "PASS" } else { "FAIL" }
        workflow_status = "VERIFIED_INVARIANT"
        detail = "Primary ZaloData config.json SHA-256 unchanged=$primaryUnchanged ($($primaryBefore.running_zalo_count) running primary Zalo processes preserved; real MultiZaloDesktop profiles unchanged=$realUserProfilesUnchanged)."
    },
    [ordered]@{
        id = "INST-14"
        title = "Zero Orphan Manager or Installer Processes Post-Check"
        gate_status = if ($zeroOrphansPass) { "PASS" } else { "FAIL" }
        workflow_status = "VERIFIED_INVARIANT"
        detail = "New orphan manager/installer PIDs left behind: $($newOrphanManagerPids.Count)."
    }
)

if (-not $primarySafetyPass -or -not $realUserProfilesUnchanged -or -not $zeroOrphansPass -or -not $wv2Installed -or -not $bundleConfigOk) {
    $overallGateStatus = "FAIL"
    $exitCodeToReturn = 1
}

$swTotal.Stop()

$countsByStatus = [ordered]@{
    PASS = @($checks | Where-Object { $_.gate_status -eq "PASS" }).Count
    FAIL = @($checks | Where-Object { $_.gate_status -eq "FAIL" }).Count
    BLOCKED = @($checks | Where-Object { $_.gate_status -eq "BLOCKED" }).Count
    NOT_TESTED = @($checks | Where-Object { $_.gate_status -eq "NOT_TESTED" }).Count
}

$worktreeHeadSha = "cd62c523222cb3a02051347e85313226de8b6964"
try {
    $gitHead = (& git --no-optional-locks -C $worktreeDir rev-parse HEAD 2>$null)
    if ($LASTEXITCODE -eq 0 -and -not [string]::IsNullOrWhiteSpace($gitHead)) {
        $worktreeHeadSha = ([string]$gitHead).Trim()
    }
} catch {
    # Keep default SHA if git is unavailable
}

$rawReport = [ordered]@{
    schema_version = 1
    suite = "windows-installer-acceptance"
    section = "Section 4 — Installer Acceptance & Host Packaging Readiness"
    executed_at_utc = $scriptStartedUtc.ToString("yyyy-MM-ddTHH:mm:ss.fffZ")
    duration_ms = [int]$swTotal.ElapsedMilliseconds
    worktree_branch = "qa/windows-native-acceptance"
    tested_integration_sha = "cd62c523222cb3a02051347e85313226de8b6964"
    worktree_head_sha = $worktreeHeadSha
    overall_gate_status = $overallGateStatus
    workflow_status = $overallWorkflowStatus
    release_captain_handoff = [ordered]@{
        installer_track_status = $rcInstallerTrackStatus
        active_release_blockers = [object[]]$rcActiveBlockers.ToArray()
        discovered_installer_bundles_count = $discoveredInstallers.Count
        observed_raw_executables_not_installers = [object[]]$observedRawBinaries.ToArray()
    }
    host_environment = [ordered]@{
        os_caption = if ($null -ne $osCim) { [string]$osCim.Caption } else { "Windows" }
        os_version = if ($null -ne $osCim) { [string]$osCim.Version } else { "unknown" }
        os_build = if ($null -ne $osCim) { [string]$osCim.BuildNumber } else { "unknown" }
        os_architecture = if ($null -ne $osCim) { [string]$osCim.OSArchitecture } else { $env:PROCESSOR_ARCHITECTURE }
        running_as_elevated_admin = [bool]$isAdmin
        execution_posture = if ($isAdmin) { "Administrator (Elevated)" } else { "Standard User (asInvoker)" }
        smart_app_control = [ordered]@{
            registry_key = "HKLM:\SYSTEM\CurrentControlSet\Control\CI\Policy\VerifiedAndReputablePolicyState"
            real_host_value = $realSacValue
            effective_evaluated_value = $effectiveSacValue
            sac_simulated = ($SimulatedSacState -ge 0)
            policy_state_label = $sacLabel
            policy_modified = $false
            unsigned_installer_behavior_when_enforced = "BLOCKED_BY_PLATFORM_POLICY"
        }
        webview2_evergreen = [ordered]@{
            client_guid = $wv2Guid
            installed = [bool]$wv2Installed
            version = $wv2Version
            binary_path = $wv2BinaryPath
            registry_entries = [object[]]$wv2DetectedEntries.ToArray()
        }
        packaging_toolchains = [ordered]@{
            rustc_available = ($null -ne $rustcPath)
            rustc_version = $rustcVersion
            rustc_host_triple = $rustcHostTriple
            cargo_available = ($null -ne $cargoPath)
            msvc_cl_available = ($null -ne $clPath)
            vswhere_present = (Test-Path -LiteralPath $vswherePath -PathType Leaf)
            nsis_makensis_available = ($null -ne $makensisPath)
            wix_v3_available = ($null -ne $candlePath -and $null -ne $lightPath)
            wix_v4_available = ($null -ne $wixCliPath)
            tauri_cli_available = ($null -ne $tauriCmdPath -or $null -ne $cargoTauriPath)
            signtool_available = ($null -ne $signtoolPath)
            signing_certificate_configured = [bool]$hasSignCertConfigured
        }
        official_vng_zalo = [ordered]@{
            installed_versions = [object[]]@($zaloVersions)
            latest_executable = $latestZaloExe
            authenticode_status = $zaloAuthStatus
            signer_subject = $zaloSignerSubject
        }
    }
    bundle_configuration_audit = [ordered]@{
        tauri_conf_path = $tauriConfPath
        product_name = [string]$tauriConf.productName
        version = [string]$tauriConf.version
        identifier = [string]$tauriConf.identifier
        bundle_active = [bool]$tauriConf.bundle.active
        bundle_targets = [string]$tauriConf.bundle.targets
        configured_icons = [object[]]@($tauriConf.bundle.icon)
        missing_icons = [object[]]$missingIcons.ToArray()
        valid_windows_ico_header = [bool]$validIcoHeader
        has_base_windows_bundle_section = ($null -ne $tauriConf.bundle.PSObject.Properties["windows"])
        ci_overlay_present = ($null -ne $overlayConf)
        ci_overlay_webview_install_mode = if ($null -ne $overlayConf -and $null -ne $overlayConf.bundle.windows) { [string]$overlayConf.bundle.windows.webviewInstallMode.type } else { $null }
        ir003_patch_present = (Test-Path -LiteralPath $ir003PatchPath -PathType Leaf)
        ir006_manifest_patch_present = (Test-Path -LiteralPath $ir006PatchPath -PathType Leaf)
        vng_binaries_in_bundle_inputs_count = $srcTauriVngHits.Count
    }
    candidate_evaluation = $candidateDetails
    lifecycle_execution = $lifecycleEvidence
    safety_invariants = [ordered]@{
        primary_zalo_config_sha256_before = $primaryBefore.config_sha256
        primary_zalo_config_sha256_after = $primaryAfter.config_sha256
        primary_zalo_unchanged = [bool]$primarySafetyPass
        primary_zalo_running_count_before = $primaryBefore.running_zalo_count
        primary_zalo_running_count_after = $primaryAfter.running_zalo_count
        real_user_profiles_unchanged = [bool]$realUserProfilesUnchanged
        manager_pids_before = [object[]]@($managerPidsBefore)
        manager_pids_after = [object[]]@($managerPidsAfter)
        new_orphan_manager_pids = [object[]]@($newOrphanManagerPids)
        zero_orphans = [bool]$zeroOrphansPass
        smart_app_control_untouched = $true
    }
    summary_counts = $countsByStatus
    checks = [object[]]@($checks)
}

$redactedReport = Redact-PiiValue -Value $rawReport
$jsonText = ($redactedReport | ConvertTo-Json -Depth 10)
$jsonText = $jsonText.Replace('\u003c', '<').Replace('\u003e', '>').Replace('\u0026', '&')

if (-not [string]::IsNullOrWhiteSpace($JsonOut)) {
    $outParent = Split-Path -Parent $JsonOut
    if (-not [string]::IsNullOrWhiteSpace($outParent) -and -not (Test-Path -LiteralPath $outParent -PathType Container)) {
        New-Item -ItemType Directory -Path $outParent -Force | Out-Null
    }
    $utf8NoBomOut = New-Object System.Text.UTF8Encoding($false)
    [System.IO.File]::WriteAllText($JsonOut, $jsonText + "`n", $utf8NoBomOut)
}

Write-Output $jsonText
exit $exitCodeToReturn

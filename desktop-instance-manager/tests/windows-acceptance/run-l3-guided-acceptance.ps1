[CmdletBinding()]
param(
    [switch]$NonInteractive,
    [AllowEmptyString()][string]$JsonOut = "",
    [AllowEmptyString()][string]$DataDir = ""
)

$ErrorActionPreference = "Stop"
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8

$startedUtc = (Get-Date).ToUniversalTime()
$sw = [System.Diagnostics.Stopwatch]::StartNew()

$dimRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$worktreeDir = (Resolve-Path (Join-Path $dimRoot "..")).Path
$worktreeParent = Split-Path -Parent $worktreeDir

if ([string]::IsNullOrWhiteSpace($DataDir)) {
    $DataDir = Join-Path $env:TEMP "dim-l3-guided-acceptance"
}

function Redact-L3Text {
    param([AllowNull()][string]$InputText)
    if ($null -eq $InputText) { return $null }
    if ($InputText.Length -eq 0) { return "" }

    $clean = $InputText

    if (-not [string]::IsNullOrWhiteSpace($worktreeParent)) {
        $escWtBack = [regex]::Escape($worktreeParent)
        $escWtFwd = [regex]::Escape(($worktreeParent -replace '\\', '/'))
        $clean = [regex]::Replace($clean, $escWtBack, "<WORKTREE_ROOT>", [System.Text.RegularExpressions.RegexOptions]::IgnoreCase)
        $clean = [regex]::Replace($clean, $escWtFwd, "<WORKTREE_ROOT>", [System.Text.RegularExpressions.RegexOptions]::IgnoreCase)
    }

    if (-not [string]::IsNullOrWhiteSpace($env:USERPROFILE)) {
        $escHomeBack = [regex]::Escape($env:USERPROFILE)
        $escHomeFwd = [regex]::Escape(($env:USERPROFILE -replace '\\', '/'))
        $clean = [regex]::Replace($clean, $escHomeBack, "<USER_HOME>", [System.Text.RegularExpressions.RegexOptions]::IgnoreCase)
        $clean = [regex]::Replace($clean, $escHomeFwd, "<USER_HOME>", [System.Text.RegularExpressions.RegexOptions]::IgnoreCase)
    }

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
        # Ignore DNS lookup failure
    }

    if (-not [string]::IsNullOrWhiteSpace($env:USERNAME) -and $env:USERNAME.Length -ge 3) {
        $escUser = [regex]::Escape($env:USERNAME)
        $clean = [regex]::Replace($clean, "\b$escUser\b", "<USER>", [System.Text.RegularExpressions.RegexOptions]::IgnoreCase)
    }

    # Scrub any accidental phone numbers or email addresses from operator notes
    $clean = [regex]::Replace($clean, '(?<!\d)(?:\+?84|0)\d{8,10}(?!\d)', '<REDACTED_PHONE>')
    $clean = [regex]::Replace($clean, '[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}', '<REDACTED_EMAIL>')

    return $clean
}

function Redact-L3Value {
    param($Value)
    if ($null -eq $Value) { return $null }
    if ($Value -is [string]) { return (Redact-L3Text -InputText $Value) }
    if ($Value -is [bool] -or $Value -is [int] -or $Value -is [long] -or $Value -is [double] -or $Value -is [decimal]) {
        return $Value
    }
    if ($Value -is [System.Collections.IDictionary]) {
        $outMap = [ordered]@{}
        foreach ($k in $Value.Keys) {
            $outMap[(Redact-L3Text -InputText ([string]$k))] = Redact-L3Value -Value $Value[$k]
        }
        return $outMap
    }
    if ($Value -is [System.Collections.IEnumerable] -and -not ($Value -is [string])) {
        $arr = New-Object System.Collections.ArrayList
        foreach ($item in $Value) {
            [void]$arr.Add((Redact-L3Value -Value $item))
        }
        return ,$arr.ToArray()
    }
    return (Redact-L3Text -InputText ([string]$Value))
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

function Get-PrimaryZaloState {
    $primaryZaloDir = Join-Path $env:APPDATA "ZaloData"
    $cfgPath = Join-Path $primaryZaloDir "config.json"
    $zaloPids = @(Get-Process -ErrorAction SilentlyContinue | Where-Object {
        $_.ProcessName -match '^(Zalo|ZaloCall|ZaloCap|zocr-host)$'
    } | Select-Object -ExpandProperty Id | Sort-Object)
    $zaloCallPids = @(Get-Process -Name "ZaloCall" -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Id | Sort-Object)

    return [ordered]@{
        zalo_data_exists = (Test-Path -LiteralPath $primaryZaloDir)
        config_sha256 = (Get-FileSha256Safe -FilePath $cfgPath)
        running_zalo_pids = [object[]]@($zaloPids)
        running_zalo_count = $zaloPids.Count
        zalo_call_helper_active = ($zaloCallPids.Count -gt 0)
        zalo_call_helper_count = $zaloCallPids.Count
    }
}

# Safety guard: DataDir must NEVER point inside primary %APPDATA%\ZaloData
$primaryZaloRoot = Join-Path $env:APPDATA "ZaloData"
if ($DataDir.TrimEnd('\', '/').ToLowerInvariant().StartsWith($primaryZaloRoot.TrimEnd('\', '/').ToLowerInvariant())) {
    throw "PROTECTED_PRIMARY_PATH: DataDir must never overlap primary %APPDATA%\ZaloData."
}

$primaryBefore = Get-PrimaryZaloState

# Read-only preflight: official VNG Zalo presence + UI boundary disclosure audit
$zaloInstallRoot = Join-Path $env:LOCALAPPDATA "Programs\Zalo"
$latestZaloVersion = $null
$zaloSignatureValid = $false
if (Test-Path -LiteralPath $zaloInstallRoot -PathType Container) {
    $vDirs = @(Get-ChildItem -LiteralPath $zaloInstallRoot -Directory -Filter "Zalo-*" -ErrorAction SilentlyContinue | Sort-Object {
        $verPart = $_.Name -replace '^Zalo-', ''
        $parsed = $null
        if ([version]::TryParse($verPart, [ref]$parsed)) { $parsed } else { [version]"0.0.0" }
    })
    if ($vDirs.Count -gt 0) {
        $latestDir = $vDirs[$vDirs.Count - 1]
        $latestZaloVersion = $latestDir.Name -replace '^Zalo-', ''
        $zExe = Join-Path $latestDir.FullName "Zalo.exe"
        if (Test-Path -LiteralPath $zExe -PathType Leaf) {
            $sig = Get-AuthenticodeSignature -LiteralPath $zExe -ErrorAction SilentlyContinue
            if ($null -ne $sig -and $sig.Status -eq "Valid" -and $sig.SignerCertificate.Subject -match 'VNG GROUP JSC') {
                $zaloSignatureValid = $true
            }
        }
    }
}

$viI18nPath = Join-Path $dimRoot "apps\desktop\ui\i18n\vi-VN.json"
$enI18nPath = Join-Path $dimRoot "apps\desktop\ui\i18n\en-US.json"
$viI18n = if (Test-Path -LiteralPath $viI18nPath -PathType Leaf) { Get-Content -LiteralPath $viI18nPath -Raw -Encoding UTF8 | ConvertFrom-Json } else { $null }
$enI18n = if (Test-Path -LiteralPath $enI18nPath -PathType Leaf) { Get-Content -LiteralPath $enI18nPath -Raw -Encoding UTF8 | ConvertFrom-Json } else { $null }
$uiDisclosesCallBoundary = (
    $null -ne $viI18n -and $null -ne $enI18n -and
    ([string]$viI18n.help.cards.commonIssues.body -match 'cuộc gọi') -and
    ([string]$enI18n.help.cards.commonIssues.body -match 'call limit')
)

$checklistDefinitions = @(
    [ordered]@{
        id = "L3-ACC-01"
        title = "Independent 2-Account Manual QR Sign-In"
        prompt = "Add Test Account A and Test Account B in Multi-Zalo, click 'Mở Zalo' / 'Open Zalo' on each, and manually scan QR codes using 2 separate authorized test Zalo accounts (zero automation)."
        privacy_rule = "Never record phone numbers, account names, QR payloads, or session tokens."
    },
    [ordered]@{
        id = "L3-ACC-02"
        title = "Simultaneous Dual-Account Coexistence & Primary Protection"
        prompt = "Confirm both Test Account A and Test Account B stay signed in and open simultaneously while any Primary Zalo session remains untouched, with distinct zalo_user_id values and DUPLICATE_ZALO_USER_ID guard active."
        privacy_rule = "Record boolean coexistence outcome only; omit numeric Zalo UIDs from notes."
    },
    [ordered]@{
        id = "L3-ACC-03"
        title = "Zero Cross-Logout & DPAPI Session Isolation Across Restart"
        prompt = "Close and reopen Multi-Zalo manager (and restart account windows); verify neither Account A, Account B, nor Primary Zalo triggers a forced logout or token invalidation in another account."
        privacy_rule = "Never inspect or copy Cookies, Local Storage, or DPAPI encrypted blobs."
    },
    [ordered]@{
        id = "L3-ACC-04"
        title = "Accurate Window Switching ('Chuyển đến' / 'Switch to window')"
        prompt = "Click 'Chuyển đến' / 'Switch to window' on Account A and then on Account B (both normal and minimized/tray states); confirm the exact matching account's main chat window comes to foreground."
        privacy_rule = "If taking UI screenshots of Multi-Zalo Dock, never include real Zalo chat lists or avatars."
    },
    [ordered]@{
        id = "L3-ACC-05"
        title = "Independent Single-Account Close ('Đóng Zalo' / 'Close Zalo')"
        prompt = "Click 'Đóng Zalo' / 'Close Zalo' on Account B only; confirm Account B closes cleanly while Account A and Primary Zalo remain open and responsive."
        privacy_rule = "Record lifecycle status only."
    },
    [ordered]@{
        id = "L3-ACC-06"
        title = "Clean Reopen With Session Preserved ('Mở Zalo' / 'Open Zalo')"
        prompt = "Click 'Mở Zalo' / 'Open Zalo' on Account B after closing it; confirm Account B reopens directly into its signed-in state without requesting QR sign-in again."
        privacy_rule = "Record session persistence status only."
    },
    [ordered]@{
        id = "L3-ACC-07"
        title = "Synthetic Test Message & Dummy File Transfer Isolation"
        prompt = "Send synthetic text 'L3-PING-TEST' and dummy file 'dummy-qa-test.txt' between Account A and Account B; verify delivery and confirm received file saves strictly inside <ProfileDir>\Documents\Zalo Received Files\."
        privacy_rule = "STRICT: Never capture message text, phone numbers, contact avatars, or chat window screenshots in evidence."
    },
    [ordered]@{
        id = "L3-ACC-08"
        title = "Voice/Video Call (PipeZCall*) & Windows Toast (com.vng.zalo vs notiWin) Boundary Verification"
        prompt = "Verify user-facing Help/docs honestly disclose VNG global named-pipe call boundary (\\.\pipe\PipeZCallRecv & PipeZCallSend held by first signed-in ZaloCall.exe; sequential single-account call handoff only) and notification split (shared com.vng.zalo Windows Action Center Toast vs per-instance in-app notiWin banner)."
        privacy_rule = "Record observed boundary behavior and disclosure clarity only."
    }
)

$recordedChecks = New-Object System.Collections.ArrayList
$overallGateStatus = "NOT_TESTED"
$workflowStatus = "AWAITING_MANUAL_L3"
$executionMode = if ($NonInteractive) { "non_interactive_preflight" } else { "interactive_human_guided" }

if ($NonInteractive) {
    foreach ($item in $checklistDefinitions) {
        [void]$recordedChecks.Add([ordered]@{
            id = $item.id
            title = $item.title
            gate_status = "NOT_TESTED"
            workflow_status = "AWAITING_MANUAL_L3"
            architectural_boundary = if ($item.id -eq "L3-ACC-08") {
                "Concurrent calls BLOCKED by vendor \\.\pipe\PipeZCall*; Windows Action Center Toast PARTIALLY_SUPPORTED (use per-instance notiWin)."
            } else {
                $null
            }
            notes = "Skipped in -NonInteractive mode; requires human tester with 2 authorized Zalo accounts (QR scan automation is prohibited)."
        })
    }
} elseif (-not [Environment]::UserInteractive) {
    $executionMode = "non_interactive_fallback_no_tty"
    foreach ($item in $checklistDefinitions) {
        [void]$recordedChecks.Add([ordered]@{
            id = $item.id
            title = $item.title
            gate_status = "NOT_TESTED"
            workflow_status = "AWAITING_MANUAL_L3"
            notes = "Host session is non-interactive; returning AWAITING_MANUAL_L3 without blocking."
        })
    }
} else {
    Write-Host "========================================================================"
    Write-Host " Multi-Zalo Desktop — Section 5 Guided L3 2-Account Acceptance Recorder"
    Write-Host "========================================================================"
    Write-Host "MANDATORY SAFETY & PRIVACY RULES:"
    Write-Host "  1. Sign in ONLY by manually scanning QR codes with 2 authorized test accounts."
    Write-Host "  2. Primary Zalo (%APPDATA%\ZaloData) is protected and NEVER modified."
    Write-Host "  3. NEVER enter phone numbers, real names, or message contents into notes."
    Write-Host "  4. NEVER capture chat screenshots, avatars, or QR codes as evidence."
    Write-Host ""

    $hostScript = Join-Path $dimRoot "scripts\dim-host.ps1"
    $launchViaHelper = Read-Host "Create & launch 2 isolated test profiles in sandbox ($DataDir) automatically now? (y/N)"
    $createdIds = New-Object System.Collections.ArrayList

    try {
        if ($launchViaHelper -match '^(?i)y(es)?$') {
            $rawA = & powershell -NoProfile -ExecutionPolicy Bypass -File $hostScript --cli --json --data-dir $DataDir -- create "Test Account A"
            $pA = $rawA | ConvertFrom-Json
            $rawB = & powershell -NoProfile -ExecutionPolicy Bypass -File $hostScript --cli --json --data-dir $DataDir -- create "Test Account B"
            $pB = $rawB | ConvertFrom-Json
            if ($pA.ok -and $pB.ok) {
                [void]$createdIds.Add([string]$pA.data.id)
                [void]$createdIds.Add([string]$pB.data.id)
                [void](& powershell -NoProfile -ExecutionPolicy Bypass -File $hostScript --cli --json --data-dir $DataDir -- launch $pA.data.id)
                [void](& powershell -NoProfile -ExecutionPolicy Bypass -File $hostScript --cli --json --data-dir $DataDir -- launch $pB.data.id)
                Write-Host "Launched Test Account A ($($pA.data.id)) and Test Account B ($($pB.data.id))."
            }
        }

        foreach ($item in $checklistDefinitions) {
            Write-Host ""
            Write-Host "[$($item.id)] $($item.title)"
            Write-Host "  Action : $($item.prompt)"
            Write-Host "  Privacy: $($item.privacy_rule)"
            $ans = Read-Host "  Result (PASS / FAIL / BLOCKED / NOT_TESTED) [default: NOT_TESTED]"
            $norm = if ([string]::IsNullOrWhiteSpace($ans)) { "NOT_TESTED" } else { $ans.Trim().ToUpperInvariant() }
            if ($norm -notin @("PASS", "FAIL", "BLOCKED", "NOT_TESTED")) {
                $norm = "NOT_TESTED"
            }
            $rawNote = Read-Host "  Redacted QA note (optional, NO phone numbers/messages)"
            [void]$recordedChecks.Add([ordered]@{
                id = $item.id
                title = $item.title
                gate_status = $norm
                workflow_status = if ($norm -eq "NOT_TESTED") { "AWAITING_MANUAL_L3" } else { "RECORDED_BY_HUMAN_TESTER" }
                notes = (Redact-L3Text -InputText $rawNote)
            })
        }
    } finally {
        if ($createdIds.Count -gt 0 -and (Test-Path -LiteralPath (Join-Path $DataDir "profiles.json"))) {
            foreach ($cid in $createdIds) {
                [void](& powershell -NoProfile -ExecutionPolicy Bypass -File $hostScript --cli --json --data-dir $DataDir -- stop $cid 2>$null)
            }
        }
    }

    $statuses = @($recordedChecks | Select-Object -ExpandProperty gate_status)
    if ($statuses -contains "FAIL") {
        $overallGateStatus = "FAIL"
        $workflowStatus = "MANUAL_L3_FAILED"
    } elseif ($statuses -contains "NOT_TESTED") {
        $overallGateStatus = "NOT_TESTED"
        $workflowStatus = "AWAITING_MANUAL_L3"
    } elseif ($statuses -contains "BLOCKED") {
        # L3-ACC-08 may record PASS (honest disclosure verified) or BLOCKED (concurrent call vendor limit)
        $nonAcc08Blocked = @($recordedChecks | Where-Object { $_.id -ne "L3-ACC-08" -and $_.gate_status -eq "BLOCKED" })
        if ($nonAcc08Blocked.Count -gt 0) {
            $overallGateStatus = "BLOCKED"
            $workflowStatus = "MANUAL_L3_BLOCKED"
        } else {
            $overallGateStatus = "PASS"
            $workflowStatus = "MANUAL_L3_VERIFIED_WITH_DISCLOSED_VENDOR_LIMITS"
        }
    } else {
        $overallGateStatus = "PASS"
        $workflowStatus = "MANUAL_L3_VERIFIED"
    }
}

$primaryAfter = Get-PrimaryZaloState
$primaryUnchanged = (
    $primaryBefore.zalo_data_exists -eq $primaryAfter.zalo_data_exists -and
    $primaryBefore.config_sha256 -eq $primaryAfter.config_sha256
)
$primaryPidsLost = @($primaryBefore.running_zalo_pids | Where-Object { $primaryAfter.running_zalo_pids -notcontains $_ })
$primaryProtectedOk = ($primaryUnchanged -and $primaryPidsLost.Count -eq 0)

$exitCode = 0
if (-not $primaryProtectedOk -or $overallGateStatus -eq "FAIL") {
    $overallGateStatus = "FAIL"
    $exitCode = 1
}

$sw.Stop()

$checksArray = $recordedChecks.ToArray()
$summaryCounts = [ordered]@{
    PASS = @($checksArray | Where-Object { $_.gate_status -eq "PASS" }).Count
    FAIL = @($checksArray | Where-Object { $_.gate_status -eq "FAIL" }).Count
    BLOCKED = @($checksArray | Where-Object { $_.gate_status -eq "BLOCKED" }).Count
    NOT_TESTED = @($checksArray | Where-Object { $_.gate_status -eq "NOT_TESTED" }).Count
}

$rawOutput = [ordered]@{
    schema_version = 1
    suite = "windows-l3-real-account-acceptance"
    section = "Section 5 — L3 Real-Account Manual QR Acceptance"
    tested_integration_sha = "cd62c523222cb3a02051347e85313226de8b6964"
    executed_at_utc = $startedUtc.ToString("yyyy-MM-ddTHH:mm:ss.fffZ")
    duration_ms = [int]$sw.ElapsedMilliseconds
    execution_mode = $executionMode
    overall_gate_status = $overallGateStatus
    workflow_status = $workflowStatus
    build_classification_while_pending = if ($overallGateStatus -eq "PASS") { "L3_MULTI_ACCOUNT_VERIFIED" } else { "INTERNAL_TEST_BUILD" }
    host_preflight = [ordered]@{
        official_vng_zalo_detected = ($null -ne $latestZaloVersion)
        official_vng_zalo_version = $latestZaloVersion
        official_vng_authenticode_valid = [bool]$zaloSignatureValid
        primary_zalo_running_count = $primaryBefore.running_zalo_count
        primary_zalocall_pipe_holder_active = [bool]$primaryBefore.zalo_call_helper_active
        ui_help_discloses_call_limit = [bool]$uiDisclosesCallBoundary
    }
    safety_and_privacy_invariants = [ordered]@{
        qr_or_otp_automated = $false
        session_tokens_extracted = $false
        chat_messages_or_pii_captured = $false
        primary_zalo_config_sha256_before = $primaryBefore.config_sha256
        primary_zalo_config_sha256_after = $primaryAfter.config_sha256
        primary_zalo_unchanged = [bool]$primaryProtectedOk
    }
    summary_counts = $summaryCounts
    checks = [object[]]$checksArray
}

$redactedOutput = Redact-L3Value -Value $rawOutput
$jsonOutText = ($redactedOutput | ConvertTo-Json -Depth 10)
$jsonOutText = $jsonOutText.Replace('\u003c', '<').Replace('\u003e', '>').Replace('\u0026', '&')

if (-not [string]::IsNullOrWhiteSpace($JsonOut)) {
    $parentDir = Split-Path -Parent $JsonOut
    if (-not [string]::IsNullOrWhiteSpace($parentDir) -and -not (Test-Path -LiteralPath $parentDir -PathType Container)) {
        New-Item -ItemType Directory -Path $parentDir -Force | Out-Null
    }
    $utf8NoBom = New-Object System.Text.UTF8Encoding($false)
    [System.IO.File]::WriteAllText($JsonOut, $jsonOutText + "`n", $utf8NoBom)
}

Write-Output $jsonOutText
exit $exitCode

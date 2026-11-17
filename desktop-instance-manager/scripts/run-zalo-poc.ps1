param(
    [string]$DataDir = (Join-Path $env:LOCALAPPDATA "MultiZaloDesktop_LivePoC"),
    [string]$OutFile = "",
    [switch]$KeepData
)

$ErrorActionPreference = "Stop"
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$repoRoot = Split-Path -Parent $scriptDir
$hostPs1 = Join-Path $scriptDir "dim-host.ps1"
if ([string]::IsNullOrWhiteSpace($OutFile)) {
    $OutFile = Join-Path $repoRoot "evidence\zalo-live-poc-result.json"
}

function Sanitize-Value {
    param($Value)
    if ($null -eq $Value) { return $null }
    if ($Value -is [string]) {
        $s = $Value
        foreach ($pair in @(
            @{ k = $env:APPDATA; r = "<APPDATA>" },
            @{ k = $env:LOCALAPPDATA; r = "<LOCALAPPDATA>" },
            @{ k = $env:USERPROFILE; r = "<USERPROFILE>" }
        )) {
            if (-not [string]::IsNullOrEmpty($pair.k)) {
                $s = [Regex]::Replace($s, [Regex]::Escape($pair.k), $pair.r, [System.Text.RegularExpressions.RegexOptions]::IgnoreCase)
            }
        }
        if (-not [string]::IsNullOrEmpty($env:USERNAME) -and $env:USERNAME.Length -ge 2) {
            $s = [Regex]::Replace($s, "\b" + [Regex]::Escape($env:USERNAME) + "\b", "<USER>", [System.Text.RegularExpressions.RegexOptions]::IgnoreCase)
        }
        return $s
    }
    if ($Value -is [System.Collections.IDictionary]) {
        $out = [ordered]@{}
        foreach ($k in $Value.Keys) {
            $out[$k] = Sanitize-Value $Value[$k]
        }
        return $out
    }
    if ($Value -is [System.Management.Automation.PSCustomObject]) {
        $out = [ordered]@{}
        foreach ($prop in $Value.PSObject.Properties) {
            $out[$prop.Name] = Sanitize-Value $prop.Value
        }
        return $out
    }
    if ($Value -is [System.Collections.IEnumerable] -and -not ($Value -is [string])) {
        $arr = @()
        foreach ($item in $Value) {
            $arr += ,(Sanitize-Value $item)
        }
        return $arr
    }
    return $Value
}

function Invoke-Host {
    param([string[]]$HostArgs)
    $all = @("--cli", "--json", "--data-dir", $DataDir) + $HostArgs
    $raw = (& powershell.exe -NoProfile -ExecutionPolicy Bypass -File $hostPs1 @all) -join "`n"
    $code = $LASTEXITCODE
    $parsed = $null
    if ($raw.Trim().StartsWith("{") -or $raw.Trim().StartsWith("[")) {
        $parsed = $raw | ConvertFrom-Json
    }
    return [PSCustomObject]@{
        ExitCode = $code
        Raw = $raw
        Json = $parsed
    }
}

function Get-FileHashSafe {
    param([string]$Path)
    if (Test-Path $Path) {
        return (Get-FileHash -Path $Path -Algorithm SHA256).Hash.ToLowerInvariant()
    }
    return $null
}

function Wait-ForWindowReady {
    param([string]$ProfileId, [bool]$ExpectRunning, [int]$TimeoutSec = 35)
    $sw = [System.Diagnostics.Stopwatch]::StartNew()
    while ($sw.Elapsed.TotalSeconds -lt $TimeoutSec) {
        $st = (Invoke-Host @("status")).Json
        $p = $st.profiles | Where-Object { $_.id -eq $ProfileId } | Select-Object -First 1
        if ($ExpectRunning) {
            if ($p -and $p.running -and $p.main_pid -gt 0 -and $p.window_handle -ne 0) {
                return [PSCustomObject]@{ Profile = $p; ElapsedMs = [int]$sw.ElapsedMilliseconds; Status = $st }
            }
        } else {
            if ($p -and (-not $p.running)) {
                return [PSCustomObject]@{ Profile = $p; ElapsedMs = [int]$sw.ElapsedMilliseconds; Status = $st }
            }
        }
        Start-Sleep -Milliseconds 500
    }
    throw "Timeout waiting for profile $ProfileId (ExpectRunning=$ExpectRunning)"
}

Write-Host "=== Multi-Zalo Desktop Live Zalo PoC ==="

$primaryZaloDirGuard = [System.IO.Path]::GetFullPath((Join-Path $env:APPDATA "ZaloData")).TrimEnd('\')
$resolvedPoCDataDir = [System.IO.Path]::GetFullPath($DataDir).TrimEnd('\')
if ($resolvedPoCDataDir.Equals($primaryZaloDirGuard, [System.StringComparison]::OrdinalIgnoreCase) -or
    $resolvedPoCDataDir.StartsWith($primaryZaloDirGuard + "\", [System.StringComparison]::OrdinalIgnoreCase)) {
    throw "PROTECTED_PRIMARY_PATH: Refusing to delete or write DataDir inside primary ZaloData ($DataDir)"
}

if (Test-Path $DataDir) {
    if (Test-Path (Join-Path $DataDir "profiles.json")) {
        $pre = Invoke-Host @("status")
        if ($pre.Json -and $pre.Json.profiles) {
            foreach ($prof in $pre.Json.profiles) {
                if ($prof.running) { Invoke-Host @("stop", $prof.id) | Out-Null }
            }
        }
    }
    Remove-Item $DataDir -Recurse -Force
}

$primaryZaloDir = Join-Path $env:APPDATA "ZaloData"
$primaryConfigPath = Join-Path $primaryZaloDir "config.json"
$primaryStartupPath = Join-Path $primaryZaloDir "startup.log"
$primaryLogPath = Join-Path $primaryZaloDir "log.log"

$primaryHashesBefore = [ordered]@{
    config_json = Get-FileHashSafe $primaryConfigPath
    startup_log = Get-FileHashSafe $primaryStartupPath
    log_log = Get-FileHashSafe $primaryLogPath
}

$baselineStatus = (Invoke-Host @("status")).Json
$discovery = (Invoke-Host @("discover")).Json
if (-not $discovery.valid) {
    throw "Official Zalo Desktop installation not found: $($discovery.error)"
}

$exeShaBefore = Get-FileHashSafe $discovery.executable
$asarShaBefore = Get-FileHashSafe $discovery.app_asar_path

Write-Host ("Discovered Zalo {0} ({1}), Primary Running={2}" -f $discovery.version, $discovery.authenticode_status, $baselineStatus.primary_zalo.is_running)

$evidence = $null

try {
    # Create 3 isolated profiles
    $pA = (Invoke-Host @("create", "PoC Alpha", "Live PoC instance 1")).Json.data
    $pB = (Invoke-Host @("create", "PoC Beta", "Live PoC instance 2")).Json.data
    $pC = (Invoke-Host @("create", "PoC Gamma", "Live PoC instance 3")).Json.data

    # Launch 2 isolated windows concurrently
    Invoke-Host @("launch", $pA.id) | Out-Null
    $wA = Wait-ForWindowReady -ProfileId $pA.id -ExpectRunning $true -TimeoutSec 35
    Invoke-Host @("launch", $pB.id) | Out-Null
    $wB = Wait-ForWindowReady -ProfileId $pB.id -ExpectRunning $true -TimeoutSec 35

    $stTwo = (Invoke-Host @("status")).Json
    $twoConcurrentOk = (($stTwo.profiles | Where-Object { $_.running }).Count -eq 2)

    # Launch 3rd isolated window concurrently (4 total if primary is running)
    Invoke-Host @("launch", $pC.id) | Out-Null
    $wC = Wait-ForWindowReady -ProfileId $pC.id -ExpectRunning $true -TimeoutSec 35

    Start-Sleep -Seconds 3
    $stThree = (Invoke-Host @("status", "--with-size")).Json
    $isoCheck = (Invoke-Host @("verify-isolation")).Json
    $diagReport = (Invoke-Host @("diagnostics")).Json

    $runningThree = @($stThree.profiles | Where-Object { $_.running })
    $threeConcurrentOk = ($runningThree.Count -eq 3 -and (@($runningThree | ForEach-Object { $_.main_pid } | Select-Object -Unique).Count -eq 3))

    # Measure RAM / CPU / Disk / Startup latency across all 3 running PoC profiles
    $resourceMetrics = @()
    foreach ($prof in $runningThree) {
        $cpuSec = 0.0
        foreach ($procId in $prof.process_ids) {
            $proc = Get-Process -Id $procId -ErrorAction SilentlyContinue
            if ($proc -and $null -ne $proc.CPU) { $cpuSec += $proc.CPU }
        }
        $resourceMetrics += [ordered]@{
            profile_id = $prof.id
            profile_name = $prof.name
            main_pid = $prof.main_pid
            process_count = $prof.process_count
            window_handle = $prof.window_handle
            working_set_mb = [math]::Round($prof.total_working_set_bytes / 1MB, 2)
            private_mb = [math]::Round($prof.total_private_bytes / 1MB, 2)
            disk_mb = [math]::Round($prof.data_size_bytes / 1MB, 2)
            cpu_seconds = [math]::Round($cpuSec, 2)
        }
    }

    # Verify per-profile isolated files on disk
    $perProfileArtifacts = @()
    foreach ($prof in $runningThree) {
        $zDir = $prof.zalo_data_path
        $perProfileArtifacts += [ordered]@{
            profile_id = $prof.id
            has_lockfile = (Test-Path (Join-Path $zDir "lockfile"))
            has_local_state = (Test-Path (Join-Path $zDir "Local State"))
            has_config_json = (Test-Path (Join-Path $zDir "config.json"))
            has_migrate_config = (Test-Path (Join-Path $zDir "migrate-config.json"))
            has_zalo_profile_id = (Test-Path (Join-Path $zDir ".zalo-profile-id"))
        }
    }

    # Test focus on PoC Alpha
    $focusRes = (Invoke-Host @("focus", $pA.id)).Json

    # Test rename while running on PoC Beta
    $renameRes = (Invoke-Host @("rename", $pB.id, "PoC Beta Renamed")).Json
    $stAfterRename = (Invoke-Host @("status")).Json
    $bAfterRename = $stAfterRename.profiles | Where-Object { $_.id -eq $pB.id } | Select-Object -First 1
    $renameWhileRunningOk = ($renameRes.ok -and $bAfterRename.running -and $bAfterRename.main_pid -eq $wB.Profile.main_pid)

    # Test selective stop of PoC Beta while Alpha, Gamma, and Primary remain alive
    $stopBRes = (Invoke-Host @("stop", $pB.id)).Json
    $wBStop = Wait-ForWindowReady -ProfileId $pB.id -ExpectRunning $false -TimeoutSec 20
    $stAfterStopB = (Invoke-Host @("status")).Json
    $aAfterStopB = $stAfterStopB.profiles | Where-Object { $_.id -eq $pA.id } | Select-Object -First 1
    $cAfterStopB = $stAfterStopB.profiles | Where-Object { $_.id -eq $pC.id } | Select-Object -First 1
    $selectiveStopOk = (
        $stopBRes.ok -and
        $aAfterStopB.running -and ($aAfterStopB.main_pid -eq $wA.Profile.main_pid) -and
        $cAfterStopB.running -and ($cAfterStopB.main_pid -eq $wC.Profile.main_pid)
    )

    # Test relaunch of PoC Beta
    Invoke-Host @("launch", $pB.id) | Out-Null
    $wBRelaunch = Wait-ForWindowReady -ProfileId $pB.id -ExpectRunning $true -TimeoutSec 35

    # Test crash recovery on PoC Gamma ONLY (never touching primary!)
    $gammaPidBeforeCrash = $cAfterStopB.main_pid
    $primaryPidsBaseline = @($baselineStatus.primary_zalo.locking_pids)
    if ($primaryPidsBaseline -contains $gammaPidBeforeCrash) {
        throw "SAFETY ABORT: PoC Gamma PID matches Primary Zalo PID!"
    }
    Stop-Process -Id $gammaPidBeforeCrash -Force -ErrorAction SilentlyContinue
    $wCCrash = Wait-ForWindowReady -ProfileId $pC.id -ExpectRunning $false -TimeoutSec 15
    $relaunchCRes = (Invoke-Host @("launch", $pC.id)).Json
    $wCRecover = Wait-ForWindowReady -ProfileId $pC.id -ExpectRunning $true -TimeoutSec 35
    $crashRecoveryOk = ($relaunchCRes.ok -and $wCRecover.Profile.running -and $wCRecover.Profile.main_pid -ne $gammaPidBeforeCrash)

    # Verify primary Zalo integrity after all PoC operations
    $finalStatusBeforeCleanup = (Invoke-Host @("status")).Json
    $primaryHashesAfter = [ordered]@{
        config_json = Get-FileHashSafe $primaryConfigPath
        startup_log = Get-FileHashSafe $primaryStartupPath
        log_log = Get-FileHashSafe $primaryLogPath
    }
    $exeShaAfter = Get-FileHashSafe $discovery.executable
    $asarShaAfter = Get-FileHashSafe $discovery.app_asar_path

    $primaryUnchanged = (
        ($primaryHashesBefore.config_json -eq $primaryHashesAfter.config_json) -and
        ($primaryHashesBefore.startup_log -eq $primaryHashesAfter.startup_log) -and
        ($primaryHashesBefore.log_log -eq $primaryHashesAfter.log_log) -and
        ($exeShaBefore -eq $exeShaAfter) -and
        ($asarShaBefore -eq $asarShaAfter)
    )
    $primaryProcessPreserved = $true
    if ($baselineStatus.primary_zalo.is_running) {
        $beforeSet = ($baselineStatus.primary_zalo.locking_pids | Sort-Object) -join ","
        $afterSet = ($finalStatusBeforeCleanup.primary_zalo.locking_pids | Sort-Object) -join ","
        $primaryProcessPreserved = ($finalStatusBeforeCleanup.primary_zalo.is_running -and ($beforeSet -eq $afterSet))
    }

    $evidence = [ordered]@{
        executed_at_utc = (Get-Date).ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ss.fffZ")
        zalo_discovery = [ordered]@{
            version = $discovery.version
            version_dir = $discovery.version_dir
            authenticode_status = $discovery.authenticode_status
            signer_subject = $discovery.signer_subject
            executable_sha256 = $exeShaBefore
            app_asar_sha256 = $asarShaBefore
        }
        primary_zalo_baseline = [ordered]@{
            is_running = $baselineStatus.primary_zalo.is_running
            process_count = $baselineStatus.primary_zalo.process_count
            hashes_before = $primaryHashesBefore
        }
        concurrency_checks = [ordered]@{
            two_concurrent_ok = $twoConcurrentOk
            three_concurrent_ok = $threeConcurrentOk
            startup_latency_ms = [ordered]@{
                alpha = $wA.ElapsedMs
                beta = $wB.ElapsedMs
                gamma = $wC.ElapsedMs
                beta_relaunch = $wBRelaunch.ElapsedMs
                gamma_crash_recover = $wCRecover.ElapsedMs
            }
            resource_metrics = $resourceMetrics
            per_profile_artifacts = $perProfileArtifacts
        }
        lifecycle_checks = [ordered]@{
            focus_ok = $focusRes.ok
            rename_while_running_ok = $renameWhileRunningOk
            selective_stop_ok = $selectiveStopOk
            selective_stop_ms = $wBStop.ElapsedMs
            relaunch_ok = $wBRelaunch.Profile.running
            crash_recovery_ok = $crashRecoveryOk
            crash_recovered_flag = $relaunchCRes.data.crash_recovered
        }
        isolation_verification = $isoCheck
        diagnostics_summary = [ordered]@{
            is_elevated = $diagReport.is_elevated
            smart_app_control_state = $diagReport.smart_app_control_state
            folder_redirection = $diagReport.folder_redirection
            disk_free_ok = $diagReport.disk_free_ok
            profiles_acl_safe = $diagReport.profiles_acl_safe
        }
        primary_integrity_checks = [ordered]@{
            primary_hashes_after = $primaryHashesAfter
            primary_files_sha256_identical = $primaryUnchanged
            primary_process_preserved = $primaryProcessPreserved
            official_binary_sha256_unchanged = ($exeShaBefore -eq $exeShaAfter -and $asarShaBefore -eq $asarShaAfter)
        }
        verification_taxonomy_l1_to_l5 = [ordered]@{
            L1_process_and_storage_isolation = if ($threeConcurrentOk -and $primaryUnchanged -and $primaryProcessPreserved) { "PASS" } else { "FAIL" }
            L2_lifecycle_and_integrity = if ($focusRes.ok -and $renameWhileRunningOk -and $selectiveStopOk -and $wBRelaunch.Profile.running -and $crashRecoveryOk -and ($exeShaBefore -eq $exeShaAfter) -and ($asarShaBefore -eq $asarShaAfter)) { "PASS" } else { "FAIL" }
            L3_authenticated_multi_account = "NOT_TESTED"
            L3_reason = "Unauthenticated automated PoC cannot claim multi-account chat/file/sync parity; use scripts/run-interactive-l3-checklist.ps1 with manual QR logins."
            L4_notifications_and_os_integration = "BLOCKED"
            L4_reason = "Window focus verified in L2, but Windows toast AUMID routing and call audio/video routing require live signed-in sessions."
            L5_installer_and_upgrade_resilience = "PARTIAL"
            L5_reason = "Official Zalo.exe/app.asar SHA-256 invariance and crash-recovery marker verified; signed installer/uninstaller and live Zalo auto-update preservation remain NOT_TESTED."
        }
    }
}
finally {
    Write-Host "Cleaning up live PoC isolated profiles..."
    if (Test-Path (Join-Path $DataDir "profiles.json")) {
        $cleanupSt = (Invoke-Host @("status")).Json
        foreach ($prof in $cleanupSt.profiles) {
            if ($prof.running) {
                Invoke-Host @("stop", $prof.id) | Out-Null
            }
            Invoke-Host @("delete", $prof.id, "--confirm") | Out-Null
        }
    }
    if (-not $KeepData -and (Test-Path $DataDir)) {
        for ($attempt = 0; $attempt -lt 5; $attempt++) {
            try {
                Remove-Item $DataDir -Recurse -Force -ErrorAction Stop
                break
            }
            catch {
                Start-Sleep -Milliseconds 300
            }
        }
    }
}

$sanitizedEvidence = Sanitize-Value $evidence
$outDir = Split-Path -Parent $OutFile
if (-not (Test-Path $outDir)) {
    New-Item -ItemType Directory -Path $outDir -Force | Out-Null
}
$sanitizedEvidence | ConvertTo-Json -Depth 10 | Set-Content -Path $OutFile -Encoding UTF8

Write-Host "Live Zalo PoC completed. Sanitized evidence saved to: $OutFile"
$sanitizedEvidence.primary_integrity_checks | Format-List
$sanitizedEvidence.lifecycle_checks | Format-List

if (-not ($sanitizedEvidence.concurrency_checks.two_concurrent_ok -and
          $sanitizedEvidence.concurrency_checks.three_concurrent_ok -and
          $sanitizedEvidence.lifecycle_checks.focus_ok -and
          $sanitizedEvidence.lifecycle_checks.rename_while_running_ok -and
          $sanitizedEvidence.lifecycle_checks.selective_stop_ok -and
          $sanitizedEvidence.lifecycle_checks.crash_recovery_ok -and
          $sanitizedEvidence.isolation_verification.all_passed -and
          $sanitizedEvidence.primary_integrity_checks.primary_files_sha256_identical -and
          $sanitizedEvidence.primary_integrity_checks.primary_process_preserved)) {
    throw "One or more Live Zalo PoC assertions failed!"
}

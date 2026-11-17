param(
    [switch]$KeepRunning,
    [string]$StorageRoot = (Join-Path $env:LOCALAPPDATA "MultiZaloDesktop_PoC")
)

$ErrorActionPreference = "Stop"

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$projectRoot = Split-Path -Parent $scriptDir
$launcherPs1 = Join-Path $projectRoot "dist\MultiZaloDesktop.ps1"
$evidenceOut = Join-Path $projectRoot "evidence\poc-verification-result.json"

function Invoke-Cli {
    param([string[]]$CliArgs)
    $all = @("--storage-root", $StorageRoot, "--cli") + $CliArgs
    $out = & powershell.exe -NoProfile -ExecutionPolicy Bypass -File $launcherPs1 @all
    $code = $LASTEXITCODE
    return [PSCustomObject]@{
        ExitCode = $code
        Raw = ($out -join "`n")
        Json = (($out -join "`n") | ConvertFrom-Json)
    }
}

function Wait-ForProfileState {
    param([string]$ProfileId, [bool]$ExpectRunning, [int]$TimeoutSec = 25)
    $sw = [System.Diagnostics.Stopwatch]::StartNew()
    while ($sw.Elapsed.TotalSeconds -lt $TimeoutSec) {
        $st = Invoke-Cli @("status")
        $p = $st.Json.Profiles | Where-Object { $_.ProfileId -eq $ProfileId } | Select-Object -First 1
        if ($ExpectRunning) {
            if ($p -and $p.IsRunning -and $p.MainPid -gt 0 -and $p.WindowHandleValue -ne 0) {
                return [PSCustomObject]@{ Profile = $p; ElapsedMs = [int]$sw.ElapsedMilliseconds; Status = $st.Json }
            }
        } else {
            if ($p -and (-not $p.IsRunning)) {
                return [PSCustomObject]@{ Profile = $p; ElapsedMs = [int]$sw.ElapsedMilliseconds; Status = $st.Json }
            }
        }
        Start-Sleep -Milliseconds 500
    }
    $finalSt = Invoke-Cli @("status")
    $finalP = $finalSt.Json.Profiles | Where-Object { $_.ProfileId -eq $ProfileId } | Select-Object -First 1
    throw "Timeout waiting for profile $ProfileId (ExpectRunning=$ExpectRunning). Last state: $($finalP | ConvertTo-Json -Compress)"
}

Write-Host "=== Multi-Zalo Desktop PoC Starting ==="
if (Test-Path $StorageRoot) {
    # Ensure no lingering PoC profiles are running before cleaning
    if (Test-Path (Join-Path $StorageRoot "profiles.json")) {
        $pre = Invoke-Cli @("status")
        foreach ($prof in $pre.Json.Profiles) {
            if ($prof.IsRunning) {
                Invoke-Cli @("stop", $prof.ProfileId) | Out-Null
            }
        }
    }
    Remove-Item $StorageRoot -Recurse -Force
}

# 1. Baseline Primary Zalo
$baseline = (Invoke-Cli @("status")).Json
$primaryStartupLog = Join-Path $env:APPDATA "ZaloData\startup.log"
$primaryLogLog = Join-Path $env:APPDATA "ZaloData\log.log"
$primaryStartupHashBefore = if (Test-Path $primaryStartupLog) { (Get-FileHash $primaryStartupLog -Algorithm SHA256).Hash } else { "" }
$primaryLogHashBefore = if (Test-Path $primaryLogLog) { (Get-FileHash $primaryLogLog -Algorithm SHA256).Hash } else { "" }
$exeHashBefore = (Get-FileHash $baseline.Installation.ElectronExePath -Algorithm SHA256).Hash

Write-Host "Primary Zalo Running: $($baseline.PrimaryZalo.IsRunning), LockingPids: $($baseline.PrimaryZalo.LockingPids -join ',')"

# 2. Create 2 PoC profiles (A and B)
$pA = (Invoke-Cli @("create", "PoC Account A", "Isolated test profile A")).Json.Data
$pB = (Invoke-Cli @("create", "PoC Account B", "Isolated test profile B")).Json.Data
Write-Host "Created Profile A ($($pA.Id)) and Profile B ($($pB.Id))"

# 3. Launch Profile A and wait for real window + lockfile
$launchA = (Invoke-Cli @("launch", $pA.Id)).Json
$waitA = Wait-ForProfileState -ProfileId $pA.Id -ExpectRunning $true -TimeoutSec 30
Write-Host "Profile A ready in $($waitA.ElapsedMs) ms: MainPid=$($waitA.Profile.MainPid), Procs=$($waitA.Profile.ProcessCount), HWND=$($waitA.Profile.WindowHandleValue), Title='$($waitA.Profile.WindowTitle)'"

# 4. Launch Profile B concurrently and wait for real window + lockfile
$launchB = (Invoke-Cli @("launch", $pB.Id)).Json
$waitB = Wait-ForProfileState -ProfileId $pB.Id -ExpectRunning $true -TimeoutSec 30
Write-Host "Profile B ready in $($waitB.ElapsedMs) ms: MainPid=$($waitB.Profile.MainPid), Procs=$($waitB.Profile.ProcessCount), HWND=$($waitB.Profile.WindowHandleValue), Title='$($waitB.Profile.WindowTitle)'"

# Give instances 2 seconds to settle files (Local State, config.json, Partitions)
Start-Sleep -Seconds 2
$concurrentStatus = (Invoke-Cli @("status", "--with-size")).Json
$isolationCheck = (Invoke-Cli @("verify-isolation")).Json

# Verify files inside each isolated profile
$pAState = $concurrentStatus.Profiles | Where-Object { $_.ProfileId -eq $pA.Id }
$pBState = $concurrentStatus.Profiles | Where-Object { $_.ProfileId -eq $pB.Id }

$pAFiles = @("lockfile", "config.json", "Local State", "migrate-config.json", "Partitions\zalo") | ForEach-Object {
    [PSCustomObject]@{ RelativePath = $_; Exists = (Test-Path (Join-Path $pAState.ZaloDataPath $_)) }
}
$pBFiles = @("lockfile", "config.json", "Local State", "migrate-config.json", "Partitions\zalo") | ForEach-Object {
    [PSCustomObject]@{ RelativePath = $_; Exists = (Test-Path (Join-Path $pBState.ZaloDataPath $_)) }
}

# 5. Stop only Profile B and verify Profile A & Primary remain running
$stopB = (Invoke-Cli @("stop", $pB.Id)).Json
$waitBStopped = Wait-ForProfileState -ProfileId $pB.Id -ExpectRunning $false -TimeoutSec 15
Start-Sleep -Seconds 1
$afterStopB = (Invoke-Cli @("status")).Json
$pAAfterStopB = $afterStopB.Profiles | Where-Object { $_.ProfileId -eq $pA.Id }

# 6. Relaunch Profile B and verify it comes back up on the same profile directory
$relaunchB = (Invoke-Cli @("launch", $pB.Id)).Json
$waitBRelaunch = Wait-ForProfileState -ProfileId $pB.Id -ExpectRunning $true -TimeoutSec 30

# 7. Check Primary Zalo integrity after PoC
$primaryStartupHashAfter = if (Test-Path $primaryStartupLog) { (Get-FileHash $primaryStartupLog -Algorithm SHA256).Hash } else { "" }
$primaryLogHashAfter = if (Test-Path $primaryLogLog) { (Get-FileHash $primaryLogLog -Algorithm SHA256).Hash } else { "" }
$exeHashAfter = (Get-FileHash $baseline.Installation.ElectronExePath -Algorithm SHA256).Hash
$finalBeforeCleanup = (Invoke-Cli @("status", "--with-size")).Json

$resultObj = [ordered]@{
    executed_at_utc = (Get-Date).ToUniversalTime().ToString("o")
    storage_root = $StorageRoot
    baseline_primary_zalo = $baseline.PrimaryZalo
    concurrent_status = $concurrentStatus
    isolation_verification = $isolationCheck
    profile_a_startup_ms = $waitA.ElapsedMs
    profile_b_startup_ms = $waitB.ElapsedMs
    profile_a_artifacts = $pAFiles
    profile_b_artifacts = $pBFiles
    selective_stop_b = [ordered]@{
        stop_result = $stopB
        profile_b_stopped_ms = $waitBStopped.ElapsedMs
        profile_a_still_running = $pAAfterStopB.IsRunning
        profile_a_same_pid = ($pAAfterStopB.MainPid -eq $waitA.Profile.MainPid)
        primary_still_running = $afterStopB.PrimaryZalo.IsRunning
        primary_same_pids = (($afterStopB.PrimaryZalo.LockingPids -join ',') -eq ($baseline.PrimaryZalo.LockingPids -join ','))
    }
    relaunch_b = [ordered]@{
        relaunch_ms = $waitBRelaunch.ElapsedMs
        new_pid = $waitBRelaunch.Profile.MainPid
        window_handle = $waitBRelaunch.Profile.WindowHandleValue
    }
    primary_integrity_checks = [ordered]@{
        primary_startup_log_unchanged = ($primaryStartupHashBefore -eq $primaryStartupHashAfter)
        primary_log_log_unchanged = ($primaryLogHashBefore -eq $primaryLogHashAfter)
        official_zalo_exe_hash_unchanged = ($exeHashBefore -eq $exeHashAfter)
        primary_still_running_at_end = $finalBeforeCleanup.PrimaryZalo.IsRunning
        primary_same_pids_at_end = (($finalBeforeCleanup.PrimaryZalo.LockingPids -join ',') -eq ($baseline.PrimaryZalo.LockingPids -join ','))
    }
}

$resultObj | ConvertTo-Json -Depth 8 | Set-Content $evidenceOut -Encoding UTF8

if (-not $KeepRunning) {
    Write-Host "Cleaning up temporary PoC instances..."
    Invoke-Cli @("stop", $pA.Id) | Out-Null
    Invoke-Cli @("stop", $pB.Id) | Out-Null
    Invoke-Cli @("delete", $pA.Id, "--confirm") | Out-Null
    Invoke-Cli @("delete", $pB.Id, "--confirm") | Out-Null
    Remove-Item $StorageRoot -Recurse -Force -ErrorAction SilentlyContinue
}

Write-Host "PoC Verification Complete. Saved evidence to $evidenceOut"
$resultObj.primary_integrity_checks | Format-List
$resultObj.selective_stop_b | Format-List

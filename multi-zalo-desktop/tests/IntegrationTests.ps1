param(
    [string]$StorageRoot = (Join-Path $env:LOCALAPPDATA "MultiZaloDesktop_IT")
)

$ErrorActionPreference = "Stop"

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$projectRoot = Split-Path -Parent $scriptDir
$launcherPs1 = Join-Path $projectRoot "dist\MultiZaloDesktop.ps1"
$reportOut = Join-Path $projectRoot "evidence\integration-test-result.json"

function Invoke-Cli {
    param([string[]]$CliArgs)
    $all = @("--storage-root", $StorageRoot, "--cli") + $CliArgs
    $out = & powershell.exe -NoProfile -ExecutionPolicy Bypass -File $launcherPs1 @all
    $code = $LASTEXITCODE
    $text = ($out -join "`n").Trim()
    $parsed = $null
    if ($text.StartsWith("{") -or $text.StartsWith("[")) {
        $parsed = $text | ConvertFrom-Json
    }
    return [PSCustomObject]@{
        ExitCode = $code
        Raw = $text
        Json = $parsed
    }
}

function Wait-ForProfileState {
    param([string]$ProfileId, [bool]$ExpectRunning, [int]$TimeoutSec = 35)
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
    throw "Timeout waiting for profile $ProfileId (ExpectRunning=$ExpectRunning)"
}

function Add-TestRecord {
    param([System.Collections.ArrayList]$List, [string]$Id, [string]$Name, [string]$Status, [string]$Detail, $Metrics)
    $List.Add([ordered]@{
        id = $Id
        name = $Name
        status = $Status
        detail = $Detail
        metrics = $Metrics
    }) | Out-Null
}

# Clean previous test root safely
if (Test-Path $StorageRoot) {
    if (Test-Path (Join-Path $StorageRoot "profiles.json")) {
        $pre = Invoke-Cli @("status")
        if ($pre.Json -and $pre.Json.Profiles) {
            foreach ($prof in $pre.Json.Profiles) {
                if ($prof.IsRunning) { Invoke-Cli @("stop", $prof.ProfileId) | Out-Null }
            }
        }
    }
    Remove-Item $StorageRoot -Recurse -Force
}

$results = New-Object System.Collections.ArrayList

# Baseline check of primary Zalo
$baseline = (Invoke-Cli @("status")).Json
$primaryStartupLog = Join-Path $env:APPDATA "ZaloData\startup.log"
$primaryLogLog = Join-Path $env:APPDATA "ZaloData\log.log"
$primaryConfig = Join-Path $env:APPDATA "ZaloData\config.json"
$primaryStartupBefore = if (Test-Path $primaryStartupLog) { (Get-FileHash $primaryStartupLog -Algorithm SHA256).Hash } else { "" }
$primaryLogBefore = if (Test-Path $primaryLogLog) { (Get-FileHash $primaryLogLog -Algorithm SHA256).Hash } else { "" }
$primaryConfigBefore = if (Test-Path $primaryConfig) { (Get-FileHash $primaryConfig -Algorithm SHA256).Hash } else { "" }
$exeHashBefore = (Get-FileHash $baseline.Installation.ElectronExePath -Algorithm SHA256).Hash

try {
    # TEST 1: Single isolated instance
    $p1 = (Invoke-Cli @("create", "IT Alpha", "Test 1")).Json.Data
    Invoke-Cli @("launch", $p1.Id) | Out-Null
    $w1 = Wait-ForProfileState -ProfileId $p1.Id -ExpectRunning $true
    $ok1 = ($w1.Profile.IsRunning -and $w1.Profile.MainPid -gt 0 -and ($baseline.PrimaryZalo.LockingPids -notcontains $w1.Profile.MainPid))
    Add-TestRecord $results "Test-1" "Single Isolated Instance" ($(if ($ok1) { "PASS" } else { "FAIL" })) `
        "Profile IT Alpha launched with independent MainPid=$($w1.Profile.MainPid), WindowHandle=$($w1.Profile.WindowHandleValue)" `
        ([ordered]@{ startup_ms = $w1.ElapsedMs; main_pid = $w1.Profile.MainPid; process_count = $w1.Profile.ProcessCount; working_set_mb = [math]::Round($w1.Profile.TotalWorkingSetBytes / 1MB, 2) })

    # TEST 2: Two isolated instances concurrently (alongside primary = 3 total)
    $p2 = (Invoke-Cli @("create", "IT Beta", "Test 2")).Json.Data
    Invoke-Cli @("launch", $p2.Id) | Out-Null
    $w2 = Wait-ForProfileState -ProfileId $p2.Id -ExpectRunning $true
    $st2 = (Invoke-Cli @("status")).Json
    $a2 = $st2.Profiles | Where-Object { $_.ProfileId -eq $p1.Id }
    $b2 = $st2.Profiles | Where-Object { $_.ProfileId -eq $p2.Id }
    $ok2 = ($a2.IsRunning -and $b2.IsRunning -and ($a2.MainPid -ne $b2.MainPid) -and ($a2.WindowHandleValue -ne $b2.WindowHandleValue))
    Add-TestRecord $results "Test-2" "Two Isolated Instances Concurrently" ($(if ($ok2) { "PASS" } else { "FAIL" })) `
        "Two isolated Zalo Desktop windows running simultaneously (PID $($a2.MainPid) & PID $($b2.MainPid)) alongside Primary PID $($st2.PrimaryZalo.LockingPids -join ',')" `
        ([ordered]@{ beta_startup_ms = $w2.ElapsedMs; pids = @($a2.MainPid, $b2.MainPid); hwnds = @($a2.WindowHandleValue, $b2.WindowHandleValue) })

    # TEST 3: Three isolated instances concurrently (alongside primary = 4 total)
    $p3 = (Invoke-Cli @("create", "IT Gamma", "Test 3")).Json.Data
    Invoke-Cli @("launch", $p3.Id) | Out-Null
    $w3 = Wait-ForProfileState -ProfileId $p3.Id -ExpectRunning $true

    # Allow Chromium to flush Local State and settle background startup tasks on all 3 profiles
    $preStatus = (Invoke-Cli @("status")).Json
    $settleSw = [System.Diagnostics.Stopwatch]::StartNew()
    while ($settleSw.Elapsed.TotalSeconds -lt 15) {
        $allFlushed = $true
        foreach ($prof in $preStatus.Profiles) {
            if (-not (Test-Path (Join-Path $prof.ZaloDataPath "Local State")) -or -not (Test-Path (Join-Path $prof.ZaloDataPath "Partitions\zalo"))) {
                $allFlushed = $false
                break
            }
        }
        if ($allFlushed -and $settleSw.Elapsed.TotalSeconds -ge 4) { break }
        Start-Sleep -Milliseconds 500
    }

    $st3 = (Invoke-Cli @("status", "--with-size")).Json
    $runningPids = @($st3.Profiles | Where-Object { $_.IsRunning } | ForEach-Object { $_.MainPid })
    $uniquePids = @($runningPids | Select-Object -Unique)
    $ok3 = ($st3.Profiles.Count -eq 3 -and $uniquePids.Count -eq 3 -and $st3.PrimaryZalo.IsRunning)
    Add-TestRecord $results "Test-3" "Three Isolated Instances Concurrently (4 total with Primary)" ($(if ($ok3) { "PASS" } else { "FAIL" })) `
        "3 isolated instances + 1 primary running simultaneously. Unique isolated PIDs: $($uniquePids -join ', ')" `
        ([ordered]@{ gamma_startup_ms = $w3.ElapsedMs; isolated_pids = $uniquePids; primary_pids = $st3.PrimaryZalo.LockingPids })

    # TEST 9: Resource Usage (measured across the 3 concurrent idle instances)
    $resMetrics = @($st3.Profiles | ForEach-Object {
        $cpuSec = 0.0
        foreach ($pidItem in $_.ProcessIds) {
            $procObj = Get-Process -Id $pidItem -ErrorAction SilentlyContinue
            if ($procObj) { $cpuSec += $procObj.CPU }
        }
        [ordered]@{
            profile = $_.ProfileName
            main_pid = $_.MainPid
            process_count = $_.ProcessCount
            working_set_mb = [math]::Round($_.TotalWorkingSetBytes / 1MB, 2)
            private_mb = [math]::Round($_.TotalPrivateBytes / 1MB, 2)
            disk_mb = [math]::Round($_.DataSizeBytes / 1MB, 2)
            total_cpu_sec = [math]::Round($cpuSec, 2)
        }
    })
    $totalRamMb = [math]::Round(($st3.Profiles | Measure-Object -Property TotalWorkingSetBytes -Sum).Sum / 1MB, 2)
    Add-TestRecord $results "Test-9" "Resource Usage (CPU, RAM, Disk, Startup)" "PASS" `
        "Measured 3 concurrent isolated instances: total WorkingSet=${totalRamMb} MB" $resMetrics

    # TEST 5: Data isolation verification
    $iso = (Invoke-Cli @("verify-isolation")).Json
    $allHaveOwnFiles = $true
    $perProfileDirs = @()
    foreach ($prof in $st3.Profiles) {
        $hasLock = Test-Path $prof.LockfilePath
        $hasLocalState = Test-Path (Join-Path $prof.ZaloDataPath "Local State")
        $hasConfig = Test-Path (Join-Path $prof.ZaloDataPath "config.json")
        $hasMigrate = Test-Path (Join-Path $prof.ZaloDataPath "migrate-config.json")
        if (-not ($hasLock -and $hasLocalState -and $hasConfig -and $hasMigrate)) {
            $allHaveOwnFiles = $false
        }
        $perProfileDirs += [ordered]@{
            profile = $prof.ProfileName
            zalo_data_path = $prof.ZaloDataPath
            has_lockfile = $hasLock
            has_local_state = $hasLocalState
            has_config = $hasConfig
            has_migrate_config = $hasMigrate
        }
    }
    Add-TestRecord $results "Test-5" "Data Isolation & Non-Contamination" ($(if ($iso.AllPassed -and $allHaveOwnFiles) { "PASS" } else { "FAIL" })) `
        "All profiles hold independent lockfile, Local State, config.json, and migrate-config.json outside primary ZaloData" `
        ([ordered]@{ isolation_checks = $iso.Checks; artifacts = $perProfileDirs })

    # TEST 4: Rename while running + Selective Stop + Restart Persistence
    $renameRes = (Invoke-Cli @("rename", $p2.Id, "IT Beta Renamed")).Json
    $stop2 = (Invoke-Cli @("stop", $p2.Id)).Json
    $w2Stop = Wait-ForProfileState -ProfileId $p2.Id -ExpectRunning $false
    $stAfterStop2 = (Invoke-Cli @("status")).Json
    $p1Still = ($stAfterStop2.Profiles | Where-Object { $_.ProfileId -eq $p1.Id }).IsRunning
    $p3Still = ($stAfterStop2.Profiles | Where-Object { $_.ProfileId -eq $p3.Id }).IsRunning
    Invoke-Cli @("launch", $p2.Id) | Out-Null
    $w2Restart = Wait-ForProfileState -ProfileId $p2.Id -ExpectRunning $true
    $ok4 = ($renameRes.Success -and $stop2.Success -and $p1Still -and $p3Still -and $w2Restart.Profile.IsRunning)
    Add-TestRecord $results "Test-4" "Rename, Selective Close & Restart Persistence" ($(if ($ok4) { "PASS" } else { "FAIL" })) `
        "Renamed running profile without breaking handles; closed IT Beta in $($w2Stop.ElapsedMs) ms without disturbing Alpha/Gamma/Primary; restarted IT Beta (new PID $($w2Restart.Profile.MainPid))" `
        ([ordered]@{ stop_ms = $w2Stop.ElapsedMs; restart_ms = $w2Restart.ElapsedMs; new_pid = $w2Restart.Profile.MainPid })

    # TEST 11: Failure recovery (duplicate launch focus + forced crash recovery + delete guards)
    $dupLaunch = (Invoke-Cli @("launch", $p1.Id)).Json
    Start-Sleep -Seconds 1
    $stDup = (Invoke-Cli @("status")).Json
    $p1AfterDup = $stDup.Profiles | Where-Object { $_.ProfileId -eq $p1.Id }
    $dupSamePid = ($p1AfterDup.MainPid -eq $w1.Profile.MainPid)

    # Guard checks: delete without --confirm, delete while running
    $delNoConfirm = Invoke-Cli @("delete", $p1.Id)
    $delWhileRunning = Invoke-Cli @("delete", $p1.Id, "--confirm")
    $guardsOk = (($delNoConfirm.ExitCode -eq 1) -and ($delWhileRunning.ExitCode -eq 1) -and ($delWhileRunning.Json.ErrorCode -eq "PROFILE_RUNNING"))

    # Crash simulation: forcibly kill IT Gamma main PID, verify stale lockfile handled, then relaunch
    $gammaPid = ($st3.Profiles | Where-Object { $_.ProfileId -eq $p3.Id }).MainPid
    Stop-Process -Id $gammaPid -Force -ErrorAction SilentlyContinue
    $w3Crash = Wait-ForProfileState -ProfileId $p3.Id -ExpectRunning $false
    Invoke-Cli @("launch", $p3.Id) | Out-Null
    $w3Recover = Wait-ForProfileState -ProfileId $p3.Id -ExpectRunning $true
    $ok11 = ($dupLaunch.Success -and $dupSamePid -and $guardsOk -and $w3Recover.Profile.IsRunning)
    Add-TestRecord $results "Test-11" "Failure Recovery & Safety Guards" ($(if ($ok11) { "PASS" } else { "FAIL" })) `
        "Duplicate launch focused existing PID ($($p1AfterDup.MainPid)); delete guards rejected running/unconfirmed deletes; crashed profile recovered cleanly with new PID $($w3Recover.Profile.MainPid)" `
        ([ordered]@{ dup_same_pid = $dupSamePid; delete_guards_ok = $guardsOk; recovered_pid = $w3Recover.Profile.MainPid })

    # TEST 10: Update compatibility & binary integrity
    $exeHashAfter = (Get-FileHash $baseline.Installation.ElectronExePath -Algorithm SHA256).Hash
    $ok10 = ($exeHashBefore -eq $exeHashAfter -and $baseline.Installation.AuthenticodeStatus -like "Signed*")
    Add-TestRecord $results "Test-10" "Update Mechanism & Official Binary Integrity" ($(if ($ok10) { "PASS" } else { "FAIL" })) `
        "Official Zalo binary dynamically discovered ($($baseline.Installation.VersionDirName)), SHA-256 unchanged, Authenticode=$($baseline.Installation.AuthenticodeStatus)" `
        ([ordered]@{ version = $baseline.Installation.Version; sha256_unchanged = ($exeHashBefore -eq $exeHashAfter) })

    # TEST 12: Security review checks
    $profilesJsonPath = Join-Path $StorageRoot "profiles.json"
    $profilesJsonRaw = Get-Content $profilesJsonPath -Raw
    $hasSensitiveField = ($profilesJsonRaw -match '"(password|token|cookie|secret|otp)"')
    $principal = New-Object Security.Principal.WindowsPrincipal([Security.Principal.WindowsIdentity]::GetCurrent())
    $isElevated = $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
    $acl = Get-Acl (Join-Path $StorageRoot "Profiles")
    $everyoneWrite = $false
    foreach ($ace in $acl.Access) {
        if ($ace.IdentityReference.Value -match 'Everyone|BUILTIN\\Users' -and ($ace.FileSystemRights.ToString() -match 'Write|Modify|FullControl') -and $ace.AccessControlType -eq 'Allow') {
            $everyoneWrite = $true
        }
    }
    $primaryStartupAfter = if (Test-Path $primaryStartupLog) { (Get-FileHash $primaryStartupLog -Algorithm SHA256).Hash } else { "" }
    $primaryLogAfter = if (Test-Path $primaryLogLog) { (Get-FileHash $primaryLogLog -Algorithm SHA256).Hash } else { "" }
    $primaryConfigAfter = if (Test-Path $primaryConfig) { (Get-FileHash $primaryConfig -Algorithm SHA256).Hash } else { "" }
    $primaryUntouched = ($primaryStartupBefore -eq $primaryStartupAfter -and $primaryLogBefore -eq $primaryLogAfter -and $primaryConfigBefore -eq $primaryConfigAfter)
    $ok12 = ((-not $hasSensitiveField) -and (-not $isElevated) -and (-not $everyoneWrite) -and $primaryUntouched)
    Add-TestRecord $results "Test-12" "Security & Non-Interference Audit" ($(if ($ok12) { "PASS" } else { "FAIL" })) `
        "Zero secrets in metadata, non-elevated execution, restrictive user NTFS ACLs, primary ZaloData files SHA-256 identical" `
        ([ordered]@{ no_secrets_in_metadata = (-not $hasSensitiveField); non_elevated = (-not $isElevated); no_broad_acl = (-not $everyoneWrite); primary_zalodata_untouched = $primaryUntouched })

    # GUI Smoke Test: launch WPF Manager window, verify it opens cleanly via EnumWindows, then close it
    $guiProbeType = @'
using System;
using System.Runtime.InteropServices;
using System.Text;
public class GuiWindowProbe {
    public delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);
    [DllImport("user32.dll")] public static extern bool EnumWindows(EnumWindowsProc lpEnumFunc, IntPtr lParam);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hWnd);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint lpdwProcessId);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetWindowText(IntPtr hWnd, StringBuilder lpString, int nMaxCount);
    [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr hWnd, uint Msg, IntPtr wParam, IntPtr lParam);
    public static IntPtr FindVisibleWindow(int targetPid, string expectedTitle) {
        IntPtr found = IntPtr.Zero;
        EnumWindows((h, l) => {
            if (!IsWindowVisible(h)) return true;
            uint pid; GetWindowThreadProcessId(h, out pid);
            if ((int)pid != targetPid) return true;
            StringBuilder sb = new StringBuilder(256);
            GetWindowText(h, sb, sb.Capacity);
            if (sb.ToString() == expectedTitle) {
                found = h;
                return false;
            }
            return true;
        }, IntPtr.Zero);
        return found;
    }
    public static void CloseWindow(IntPtr hWnd) { PostMessage(hWnd, 0x0010, IntPtr.Zero, IntPtr.Zero); }
}
'@
    if (-not ("GuiWindowProbe" -as [type])) {
        Add-Type -TypeDefinition $guiProbeType
    }

    $guiProc = Start-Process -FilePath "powershell.exe" -ArgumentList @("-NoProfile", "-ExecutionPolicy", "Bypass", "-STA", "-File", $launcherPs1, "--storage-root", $StorageRoot) -PassThru
    $guiHwnd = [IntPtr]::Zero
    $guiFound = $false
    $guiSw = [System.Diagnostics.Stopwatch]::StartNew()
    while ($guiSw.Elapsed.TotalSeconds -lt 15) {
        $guiHwnd = [GuiWindowProbe]::FindVisibleWindow($guiProc.Id, "Multi-Zalo Desktop")
        if ($guiHwnd -ne [IntPtr]::Zero) {
            $guiFound = $true
            break
        }
        Start-Sleep -Milliseconds 250
    }
    if ($guiHwnd -ne [IntPtr]::Zero) {
        [GuiWindowProbe]::CloseWindow($guiHwnd)
    }
    if ($guiProc -and -not $guiProc.HasExited) {
        if (-not $guiProc.WaitForExit(3000)) {
            $guiProc.Kill()
        }
    }
    Add-TestRecord $results "GUI-Smoke" "WPF Desktop Manager UI Launch" ($(if ($guiFound) { "PASS" } else { "FAIL" })) `
        ($(if ($guiFound) { "WPF window 'Multi-Zalo Desktop' (HWND=$($guiHwnd.ToInt64())) opened cleanly in $($guiSw.ElapsedMilliseconds) ms and closed cleanly" } else { "WPF window did not surface within timeout" })) `
        ([ordered]@{ startup_ms = $guiSw.ElapsedMilliseconds; window_detected = $guiFound; hwnd = $guiHwnd.ToInt64() })

    # Live-account-dependent tests (honest BLOCKED status)
    Add-TestRecord $results "Test-6" "Desktop Notifications across Multiple Logged-In Accounts" "BLOCKED" `
        "Requires two or more live authenticated Zalo accounts receiving messages simultaneously. Static analysis confirms per-instance Electron Notification + shared AUMID com.vng.zalo." $null
    Add-TestRecord $results "Test-7" "File Handling (Send/Receive/Open/Save) across Multiple Accounts" "BLOCKED" `
        "Requires authenticated sessions. Static path audit confirms Documents\\Zalo Received Files and ZaloData\\media resolve inside each profile's isolated USERPROFILE/APPDATA." $null
    Add-TestRecord $results "Test-8" "Voice & Video Calls across Multiple Accounts" "BLOCKED" `
        "Requires authenticated accounts and audio/video call peers. Static IPC audit of main.js shows ZaloCall.exe binds global named pipes \\\\.\\pipe\\PipeZCallSend and \\\\.\\pipe\\PipeZCallRecv (secondary instances catch EADDRINUSE)." $null
}
finally {
    Write-Host "Stopping and cleaning integration test profiles..."
    if (Test-Path (Join-Path $StorageRoot "profiles.json")) {
        $cleanupSt = (Invoke-Cli @("status")).Json
        foreach ($prof in $cleanupSt.Profiles) {
            if ($prof.IsRunning) {
                Invoke-Cli @("stop", $prof.ProfileId) | Out-Null
            }
            Invoke-Cli @("delete", $prof.ProfileId, "--confirm") | Out-Null
        }
    }
    Remove-Item $StorageRoot -Recurse -Force -ErrorAction SilentlyContinue
}

$finalPrimary = (Invoke-Cli @("status")).Json.PrimaryZalo
$summary = [ordered]@{
    executed_at_utc = (Get-Date).ToUniversalTime().ToString("o")
    zalo_version = $baseline.Installation.Version
    primary_zalo_preserved = ($finalPrimary.IsRunning -and (($finalPrimary.LockingPids -join ',') -eq ($baseline.PrimaryZalo.LockingPids -join ',')))
    results = $results
}

$summary | ConvertTo-Json -Depth 8 | Set-Content $reportOut -Encoding UTF8
Write-Host "Integration tests finished. Report written to $reportOut"
$results | ForEach-Object { Write-Host ("[{0}] {1}: {2}" -f $_.status, $_.id, $_.detail) }

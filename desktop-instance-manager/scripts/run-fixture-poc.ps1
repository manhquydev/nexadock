param(
    [string]$DataDir = (Join-Path $env:LOCALAPPDATA "MultiZaloDesktop_FixturePoC"),
    [switch]$KeepData
)

$ErrorActionPreference = "Stop"
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$repoRoot = Split-Path -Parent $scriptDir
$hostPs1 = Join-Path $scriptDir "dim-host.ps1"
$fixtureScript = Join-Path $repoRoot "fixtures\test-desktop-app\fixture-app.mjs"

function Invoke-Host {
    param([string[]]$HostArgs)
    $all = @("--cli", "--json", "--data-dir", $DataDir, "--node-fixture", "--app-path", $fixtureScript) + $HostArgs
    $raw = (& powershell.exe -NoProfile -ExecutionPolicy Bypass -File $hostPs1 @all) -join "`n"
    $code = $LASTEXITCODE
    return [PSCustomObject]@{
        ExitCode = $code
        Raw = $raw
        Json = ($raw | ConvertFrom-Json)
    }
}

Write-Host "=== Multi-Zalo Desktop Fixture PoC ==="
if (Test-Path $DataDir) {
    if (Test-Path (Join-Path $DataDir "profiles.json")) {
        $pre = Invoke-Host @("status")
        foreach ($p in $pre.Json.profiles) {
            if ($p.running) { Invoke-Host @("stop", $p.id) | Out-Null }
        }
    }
    Remove-Item $DataDir -Recurse -Force
}

try {
    $disc = Invoke-Host @("discover")
    if (-not $disc.Json.valid) { throw "Fixture discovery failed: $($disc.Raw)" }

    $pA = (Invoke-Host @("create", "Đức - Công việc")).Json.data
    $pB = (Invoke-Host @("create", "Cá nhân - Test")).Json.data
    $pC = (Invoke-Host @("create", "Hỗ trợ khách hàng")).Json.data

    $lA = (Invoke-Host @("launch", $pA.id)).Json
    $lB = (Invoke-Host @("launch", $pB.id)).Json
    $lC = (Invoke-Host @("launch", $pC.id)).Json
    if (-not ($lA.ok -and $lB.ok -and $lC.ok)) { throw "Failed launching 3 concurrent fixture instances" }

    $st = (Invoke-Host @("status", "--with-size")).Json
    $iso = (Invoke-Host @("verify-isolation")).Json
    if (-not $iso.all_passed) { throw "verify-isolation failed: $($iso | ConvertTo-Json -Depth 5)" }

    $focusA = (Invoke-Host @("focus", $pA.id)).Json
    $renameB = (Invoke-Host @("rename", $pB.id, "Cá nhân (Renamed)")).Json
    $stopB = (Invoke-Host @("stop", $pB.id)).Json
    $backupB = (Invoke-Host @("backup", $pB.id)).Json
    $restoreB = (Invoke-Host @("restore", $backupB.data.backup_dir)).Json

    Write-Host "Fixture PoC PASSED: 3 concurrent instances, isolation=$($iso.all_passed), focus=$($focusA.ok), rename=$($renameB.ok), backup=$($backupB.ok), restore=$($restoreB.ok)"
}
finally {
    if (Test-Path (Join-Path $DataDir "profiles.json")) {
        $cleanup = Invoke-Host @("status")
        foreach ($p in $cleanup.Json.profiles) {
            if ($p.running) { Invoke-Host @("stop", $p.id) | Out-Null }
            Invoke-Host @("delete", $p.id, "--confirm") | Out-Null
        }
    }
    if (-not $KeepData -and (Test-Path $DataDir)) {
        Remove-Item $DataDir -Recurse -Force -ErrorAction SilentlyContinue
    }
}

param(
    [int]$Cycles = 100,
    [int]$MetricsRuns = 10,
    [string]$Seed = "0x20261117",
    [string]$Suite = "all",
    [switch]$SkipMetrics,
    [switch]$FailOnProductDefects
)

$ErrorActionPreference = "Stop"
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$crossPlatformDir = Split-Path -Parent $scriptDir
$dimRoot = Split-Path -Parent (Split-Path -Parent $crossPlatformDir)

# Auto-augment PATH with user-scope rustup & WinLibs MinGW-w64 if not already present
$cargoBin = Join-Path $env:USERPROFILE ".cargo\bin"
if ((Test-Path $cargoBin) -and ($env:PATH -notlike "*$cargoBin*")) {
    $env:PATH = "$cargoBin;$env:PATH"
}
$winLibs = Get-ChildItem "$env:LOCALAPPDATA\Microsoft\WinGet\Packages\BrechtSanders.WinLibs*" -Recurse -Filter "gcc.exe" -ErrorAction SilentlyContinue | Select-Object -First 1
if ($winLibs -and ($env:PATH -notlike "*$($winLibs.DirectoryName)*")) {
    $env:PATH = "$($winLibs.DirectoryName);$env:PATH"
}

Push-Location $dimRoot
try {
    Write-Host "=== [Windows Host] 1/4 Building synthetic desktop fixture (xf-desktop-fixture.exe) ==="
    & cargo build --manifest-path fixtures/cross-platform/Cargo.toml --release
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

    Write-Host "=== [Windows Host] 2/4 Building native dim-cli.exe (--locked) ==="
    & cargo build -p dim-test-fixture --bin dim-cli --release --locked
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

    Write-Host "=== [Windows Host] 3/4 Building cross-platform reliability runner ==="
    & cargo build --manifest-path tests/cross-platform/Cargo.toml --release --bin xf-test-runner
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

    $reportsDir = Join-Path $dimRoot "plans\reports\reliability"
    New-Item -ItemType Directory -Force -Path $reportsDir | Out-Null

    $runnerExe = Join-Path $crossPlatformDir "target\release\xf-test-runner.exe"
    $runnerArgs = @(
        "--suite", $Suite,
        "--cycles", "$Cycles",
        "--metrics-runs", "$MetricsRuns",
        "--seed", $Seed,
        "--json-out", (Join-Path $reportsDir "windows-host-execution-evidence.json"),
        "--metrics-out", (Join-Path $reportsDir "metrics-benchmark-windows.json"),
        "--defects-out", (Join-Path $reportsDir "defect-reproductions.json")
    )
    if ($SkipMetrics) { $runnerArgs += "--skip-metrics" }
    if ($FailOnProductDefects) { $runnerArgs += "--fail-on-product-defects" }

    Write-Host "=== [Windows Host] 4/4 Executing Cross-Platform Reliability & Contract Suites ==="
    & $runnerExe @runnerArgs
    exit $LASTEXITCODE
}
finally {
    Pop-Location
}

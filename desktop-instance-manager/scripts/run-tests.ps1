param(
    [switch]$IncludeLivePoc
)

$ErrorActionPreference = "Stop"
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$repoRoot = Split-Path -Parent $scriptDir
$fixtureTests = Join-Path $repoRoot "tests\run-fixture-tests.mjs"

Write-Host "=== Running Fixture Integration & Security Test Suite ==="
& node $fixtureTests
if ($LASTEXITCODE -ne 0) {
    exit $LASTEXITCODE
}

if ($IncludeLivePoc) {
    Write-Host "=== Running Live Zalo Desktop PoC ==="
    & powershell.exe -NoProfile -ExecutionPolicy Bypass -File (Join-Path $scriptDir "run-zalo-poc.ps1")
    if ($LASTEXITCODE -ne 0) {
        exit $LASTEXITCODE
    }
}

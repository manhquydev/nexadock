$ErrorActionPreference = "Stop"

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$projectRoot = Split-Path -Parent $scriptDir
$testScript = Join-Path $projectRoot "tests\IntegrationTests.ps1"

& powershell.exe -NoProfile -ExecutionPolicy Bypass -File $testScript
exit $LASTEXITCODE

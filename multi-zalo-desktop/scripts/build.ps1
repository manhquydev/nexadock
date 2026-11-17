$ErrorActionPreference = "Stop"

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$projectRoot = Split-Path -Parent $scriptDir
$srcDir = Join-Path $projectRoot "src"
$distDir = Join-Path $projectRoot "dist"
$outExe = Join-Path $distDir "MultiZaloDesktop.exe"

New-Item -ItemType Directory -Path $distDir -Force | Out-Null

$csc = Join-Path $env:WINDIR "Microsoft.NET\Framework64\v4.0.30319\csc.exe"
if (-not (Test-Path $csc)) {
    throw "C# compiler not found at $csc"
}

$fxDir = Join-Path $env:WINDIR "Microsoft.NET\Framework64\v4.0.30319"
$wpfDir = Join-Path $fxDir "WPF"

$sources = @(
    (Join-Path $srcDir "core\ProfileModels.cs"),
    (Join-Path $srcDir "core\ProfileManager.cs"),
    (Join-Path $srcDir "core\InstanceManager.cs"),
    (Join-Path $srcDir "core\DiagnosticsService.cs"),
    (Join-Path $srcDir "platform\PlatformInterfaces.cs"),
    (Join-Path $srcDir "platform\WindowsPlatformAdapter.cs"),
    (Join-Path $srcDir "platform\MacOSPlatformAdapter.cs"),
    (Join-Path $srcDir "platform\LinuxPlatformAdapter.cs"),
    (Join-Path $srcDir "ui\MainWindow.cs"),
    (Join-Path $srcDir "ui\Program.cs")
)

$refs = @(
    (Join-Path $fxDir "System.dll"),
    (Join-Path $fxDir "System.Core.dll"),
    (Join-Path $fxDir "System.Drawing.dll"),
    (Join-Path $fxDir "System.Web.Extensions.dll"),
    (Join-Path $fxDir "System.Xaml.dll"),
    (Join-Path $wpfDir "PresentationFramework.dll"),
    (Join-Path $wpfDir "PresentationCore.dll"),
    (Join-Path $wpfDir "WindowsBase.dll")
)

$cscArgs = @(
    "/nologo",
    "/target:winexe",
    "/platform:x64",
    "/optimize+",
    "/warn:4",
    "/out:$outExe"
)
foreach ($r in $refs) {
    $cscArgs += "/reference:$r"
}
$cscArgs += $sources

Write-Host "Compiling MultiZaloDesktop.exe..."
& $csc @cscArgs
if ($LASTEXITCODE -ne 0) {
    throw "csc.exe failed with exit code $LASTEXITCODE"
}

$item = Get-Item $outExe
$hash = (Get-FileHash $outExe -Algorithm SHA256).Hash.ToLower()
Write-Host "BUILD SUCCEEDED: $($item.FullName) ($($item.Length) bytes, SHA256=$hash)"

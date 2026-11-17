param(
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$PassThruArgs
)

$ErrorActionPreference = "Stop"

# Ensure STA apartment state for WPF when invoked in GUI mode
$isCli = $false
foreach ($a in $PassThruArgs) {
    if ($a -eq "--cli") { $isCli = $true; break }
}

if (-not $isCli -and [System.Threading.Thread]::CurrentThread.GetApartmentState() -ne [System.Threading.ApartmentState]::STA) {
    $argList = @("-NoProfile", "-ExecutionPolicy", "Bypass", "-STA", "-File", $PSCommandPath)
    if ($PassThruArgs) { $argList += $PassThruArgs }
    $proc = Start-Process -FilePath "powershell.exe" -ArgumentList $argList -PassThru
    exit 0
}

$distDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$projectRoot = Split-Path -Parent $distDir
$srcDir = Join-Path $projectRoot "src"

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

$fxDir = Join-Path $env:WINDIR "Microsoft.NET\Framework64\v4.0.30319"
$wpfDir = Join-Path $fxDir "WPF"

$refs = @(
    "System.dll",
    "System.Core.dll",
    "System.Drawing.dll",
    "System.Web.Extensions.dll",
    "System.Xaml.dll",
    (Join-Path $wpfDir "PresentationFramework.dll"),
    (Join-Path $wpfDir "PresentationCore.dll"),
    (Join-Path $wpfDir "WindowsBase.dll")
)

if (-not ("MultiZaloDesktop.UI.Program" -as [type])) {
    Add-Type -Path $sources -ReferencedAssemblies $refs
}

$argArray = @()
if ($PassThruArgs) {
    $argArray = [string[]]$PassThruArgs
}

$exitCode = [MultiZaloDesktop.UI.Program]::Main($argArray)
exit $exitCode

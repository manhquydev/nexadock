[CmdletBinding()]
param(
    [string]$DataDir = (Join-Path $env:TEMP "dim-l3-checklist"),
    [string]$OutFile = (Join-Path (Split-Path -Parent $PSScriptRoot) "evidence\l3-interactive-checklist-result.json")
)

$ErrorActionPreference = "Stop"

Write-Host "=================================================================="
Write-Host " MultiZaloDesktop Human-Guided L3/L4 Interactive Verification"
Write-Host "=================================================================="
Write-Host "Safety boundary:"
Write-Host " - User scans QR codes manually inside official Zalo Desktop windows."
Write-Host " - No OTP automation, no token extraction, no binary patching."
Write-Host " - Unchecked steps are recorded honestly as NOT_TESTED or BLOCKED."
Write-Host ""

$hostScript = Join-Path $PSScriptRoot "dim-host.ps1"
if (-not (Test-Path $hostScript)) {
    throw "Missing host script: $hostScript"
}

function Invoke-DimHost {
    param([string[]]$CmdArgs)
    $raw = & powershell -NoProfile -ExecutionPolicy Bypass -File $hostScript --cli --json --data-dir $DataDir -- @CmdArgs
    $parsed = $raw | ConvertFrom-Json
    return $parsed
}

function Sanitize-OperatorNote {
    param([string]$Text)
    if ([string]::IsNullOrWhiteSpace($Text)) { return "" }
    $clean = $Text
    foreach ($val in @($env:USERPROFILE, $env:LOCALAPPDATA, $env:APPDATA)) {
        if (-not [string]::IsNullOrWhiteSpace($val)) {
            $clean = $clean.Replace($val, "<USER_HOME>")
        }
    }
    if (-not [string]::IsNullOrWhiteSpace($env:USERNAME) -and $env:USERNAME.Length -ge 3) {
        $escaped = [regex]::Escape($env:USERNAME)
        $clean = [regex]::Replace($clean, "\b$escaped\b", "<USER>", [System.Text.RegularExpressions.RegexOptions]::IgnoreCase)
    }
    return $clean.Trim()
}

function Ask-CheckResult {
    param(
        [string]$Id,
        [string]$Prompt
    )
    Write-Host ""
    Write-Host "[$Id] $Prompt"
    $ans = Read-Host "Enter result (PASS / FAIL / BLOCKED / NOT_TESTED)"
    $norm = if ($null -eq $ans) { "NOT_TESTED" } else { $ans.Trim().ToUpperInvariant() }
    if ($norm -notin @("PASS", "FAIL", "BLOCKED", "NOT_TESTED")) {
        $norm = "NOT_TESTED"
    }
    $rawNotes = Read-Host "Optional notes (do not include personal names or account phone numbers)"
    $notes = Sanitize-OperatorNote $rawNotes
    return [ordered]@{
        id = $Id
        description = $Prompt
        status = $norm
        notes = $notes
    }
}

$checks = @()

try {
    Write-Host "Creating two isolated profiles for manual QR sign-in test..."
    $p1 = Invoke-DimHost @("create", "Account Alpha")
    $p2 = Invoke-DimHost @("create", "Account Beta")
    if (-not $p1.ok -or -not $p2.ok) {
        throw "Failed to create checklist profiles."
    }

    Write-Host "Launching Account Alpha ($($p1.data.id)) and Account Beta ($($p2.data.id))..."
    $l1 = Invoke-DimHost @("launch", $p1.data.id)
    $l2 = Invoke-DimHost @("launch", $p2.data.id)
    if (-not $l1.ok -or -not $l2.ok) {
        throw "Failed to launch checklist profiles."
    }

    $checks += Ask-CheckResult -Id "L3-1" -Prompt "Scan different authorized Zalo QR codes in Account Alpha and Account Beta. Are both accounts signed in simultaneously while primary Zalo stays untouched?"
    $checks += Ask-CheckResult -Id "L3-2" -Prompt "Verify account-lock uniqueness guard: restart via host and confirm profiles.json detected distinct zalo_user_id values with zero overlap."
    $checks += Ask-CheckResult -Id "L3-3" -Prompt "Send messages, images, and files between accounts. Confirm delivery and local attachment isolation inside each profile data dir."
    $checks += Ask-CheckResult -Id "L3-4" -Prompt "Close Account Beta, relaunch Account Beta, and confirm its session persists without logging out Account Alpha or Primary Zalo."
    $checks += Ask-CheckResult -Id "L4-1" -Prompt "Send an incoming message to Account Alpha while unfocused: verify notification sound/banner and click-to-focus routing."
    $checks += Ask-CheckResult -Id "L4-2" -Prompt "Place or receive a test voice/video call on an isolated instance to check microphone/camera access and audio routing."

    $iso = Invoke-DimHost @("verify-isolation")
    $result = [ordered]@{
        executed_at_utc = (Get-Date).ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ss.fffZ")
        verification_mode = "human_guided_interactive_checklist"
        isolation_check_ok = $iso.ok
        checks = $checks
    }

    $outDir = Split-Path -Parent $OutFile
    if (-not (Test-Path $outDir)) {
        New-Item -ItemType Directory -Path $outDir -Force | Out-Null
    }
    $result | ConvertTo-Json -Depth 8 | Set-Content -Path $OutFile -Encoding UTF8
    Write-Host "Saved checklist result to $OutFile"
}
finally {
    Write-Host "Stopping temporary checklist instances (profiles kept in $DataDir if needed)..."
    if (Test-Path (Join-Path $DataDir "profiles.json")) {
        $st = Invoke-DimHost @("status")
        if ($st.ok) {
            foreach ($p in $st.data.profiles) {
                if ($p.running) {
                    Invoke-DimHost @("stop", $p.id) | Out-Null
                }
            }
        }
    }
}

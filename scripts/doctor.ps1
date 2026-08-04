#Requires -Version 7.0
$ErrorActionPreference = 'Continue'

. "$PSScriptRoot\lib\ipp-printer-lib.ps1"

$fail = 0

function Test-Line([string]$Name, [bool]$Ok, [string]$Hint = '') {
    $status = if ($Ok) { 'OK' } else { 'MISSING' }
    if (-not $Ok) { $script:fail++ }
    Write-Host "$status : $Name $(if ($Hint) { "— $Hint" })"
}

Write-Host '=== fake-printer doctor (Rust) ==='

$exeOk = Test-Path -LiteralPath $script:IppPrinterExe
if (-not $exeOk) {
    $exeOk = Test-Path -LiteralPath $script:IppPrinterReleaseExe
}
Test-Line 'fake-printer.exe' $exeOk 'pwsh scripts/setup.ps1 (needs Rust: https://rustup.rs)'

$gsOk = (Test-CommandExists gswin64c) -or (Test-CommandExists gswin32c) -or (Test-CommandExists gs)
if ($gsOk) {
    Write-Host 'OK : ghostscript'
}
else {
    Write-Host 'WARN : ghostscript — needed for PostScript jobs only'
}

Test-Line 'install root' (Test-Path -LiteralPath $script:IppPrinterInstallRoot) 'scripts/setup.ps1'

$envOk = Test-Path -LiteralPath $script:IppPrinterEnvFile
if (-not $envOk) {
    # Legacy layout kept .env inside the engine clone.
    $envOk = Test-Path -LiteralPath (Join-Path $script:IppPrinterInstallRoot 'paperlessprinter\.env')
}
Test-Line '.env' $envOk 'scripts/setup.ps1'

$manifest = Read-InstallManifest
if ($manifest) {
    Write-Host "OK : manifest spool=$($manifest.spool_dir)"
    Test-Line 'spool dir writable' (Test-Path -LiteralPath $manifest.spool_dir) 'check path / permissions'
    Write-Host "       ipp_url=$($manifest.ipp_url)"
}
else {
    Test-Line 'install.json' $false 'scripts/setup.ps1'
}

# Program-scoped allow rules (created when Windows prompts on first run)
# satisfy the same need as the named port rules.
$fwProgram = @(
    Get-NetFirewallRule -DisplayName 'fake-printer.exe' -ErrorAction SilentlyContinue |
        Where-Object { $_.Enabled -eq $true -and $_.Direction -eq 'Inbound' -and $_.Action -eq 'Allow' }
).Count -gt 0

$fw8631 = $fwProgram -or (@(
    Get-NetFirewallRule -DisplayName 'FakePrinter-IPP-TCP' -ErrorAction SilentlyContinue
    Get-NetFirewallRule -DisplayName 'LenovoIppPrinter-IPP-TCP' -ErrorAction SilentlyContinue
).Count -gt 0)
Test-Line 'firewall TCP 8631' $fw8631 'scripts/open-firewall.ps1 (admin)'

$fwMdnsSystem = @(
    Get-NetFirewallRule -Direction Inbound -Action Allow -ErrorAction SilentlyContinue |
        Where-Object { $_.Enabled -eq $true -and $_.DisplayName -match '^mDNS' }
).Count -gt 0

$fw5353 = $fwProgram -or $fwMdnsSystem -or (@(
    Get-NetFirewallRule -DisplayName 'FakePrinter-mDNS-UDP' -ErrorAction SilentlyContinue
    Get-NetFirewallRule -DisplayName 'LenovoIppPrinter-mDNS-UDP' -ErrorAction SilentlyContinue
).Count -gt 0)
Test-Line 'firewall UDP 5353' $fw5353 'scripts/open-firewall.ps1 (admin)'

Test-Line 'start-fake-printer.bat' (Test-Path -LiteralPath $script:IppPrinterStartBat) 'scripts/setup.ps1'

$listen = @(Get-NetTCPConnection -LocalPort 8631 -State Listen -ErrorAction SilentlyContinue).Count -gt 0
if ($listen) {
    Write-Host 'OK : server listening :8631'
    try {
        $health = Invoke-WebRequest -Uri 'http://127.0.0.1:8631/healthz' -UseBasicParsing -TimeoutSec 3
        Write-Host "OK : healthz $($health.StatusCode)"
    }
    catch {
        Write-Host 'WARN : port open but /healthz not answering'
    }
}
else {
    Write-Host 'INFO : server not running (start start-fake-printer.bat)'
}

$legacyTask = $null -ne (Get-ScheduledTask -TaskName 'LenovoIppPrinter' -ErrorAction SilentlyContinue)
if ($legacyTask) {
    Write-Host 'WARN : legacy scheduled task exists — run scripts/setup.ps1 to remove'
}

Write-Host ''
if ($fail -eq 0) {
    Write-Host 'RESULT: OK'
    exit 0
}
Write-Host "RESULT: $fail issue(s) — run scripts/setup.ps1"
exit 1

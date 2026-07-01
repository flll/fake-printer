#Requires -Version 7.0
$ErrorActionPreference = 'Continue'

. "$PSScriptRoot\lib\ipp-printer-lib.ps1"

$fail = 0

function Test-Line([string]$Name, [bool]$Ok, [string]$Hint = '') {
    $status = if ($Ok) { 'OK' } else { 'MISSING' }
    if (-not $Ok) { $script:fail++ }
    Write-Host "$status : $Name $(if ($Hint) { "— $Hint" })"
}

Write-Host '=== fake-printer doctor ==='

Test-Line 'python' (Test-CommandExists python) 'winget install Python.Python.3.12'
if (Test-CommandExists python) {
    python --version 2>&1 | ForEach-Object { Write-Host "       $_" }
}

$gsOk = (Test-CommandExists gswin64c) -or (Test-CommandExists gswin32c) -or (Test-CommandExists gs)
if ($gsOk) {
    Write-Host 'OK : ghostscript'
}
else {
    Write-Host 'WARN : ghostscript — needed for PostScript jobs only'
}

Test-Line 'install root' (Test-Path -LiteralPath $script:IppPrinterInstallRoot) 'scripts/setup.ps1'
Test-Line 'venv python' (Test-Path -LiteralPath $script:IppPrinterVenvPython) 'scripts/setup.ps1'
Test-Line 'paperlessprinter' (Test-Path -LiteralPath (Join-Path $script:IppPrinterRepoDir 'server.py')) 'scripts/setup.ps1'
Test-Line '.env' (Test-Path -LiteralPath (Join-Path $script:IppPrinterRepoDir '.env')) 'scripts/setup.ps1'

$manifest = Read-InstallManifest
if ($manifest) {
    Write-Host "OK : manifest spool=$($manifest.spool_dir)"
    Test-Line 'spool dir writable' (Test-Path -LiteralPath $manifest.spool_dir) 'check path / permissions'
    Write-Host "       ipp_url=$($manifest.ipp_url)"
}
else {
    Test-Line 'install.json' $false 'scripts/setup.ps1'
}

$fw8631 = @(
    Get-NetFirewallRule -DisplayName 'FakePrinter-IPP-TCP' -ErrorAction SilentlyContinue
    Get-NetFirewallRule -DisplayName 'LenovoIppPrinter-IPP-TCP' -ErrorAction SilentlyContinue
).Count -gt 0
Test-Line 'firewall TCP 8631' $fw8631 'scripts/open-firewall.ps1 (admin)'

$fw5353 = @(
    Get-NetFirewallRule -DisplayName 'FakePrinter-mDNS-UDP' -ErrorAction SilentlyContinue
    Get-NetFirewallRule -DisplayName 'LenovoIppPrinter-mDNS-UDP' -ErrorAction SilentlyContinue
).Count -gt 0
Test-Line 'firewall UDP 5353' $fw5353 'scripts/open-firewall.ps1 (admin)'

Test-Line 'start-fake-printer.bat' (Test-Path -LiteralPath $script:IppPrinterStartBat) 'scripts/setup.ps1'

$listen = @(Get-NetTCPConnection -LocalPort 8631 -State Listen -ErrorAction SilentlyContinue).Count -gt 0
if ($listen) {
    Write-Host 'OK : server listening :8631'
}
else {
    Write-Host 'INFO : server not running (start start-fake-printer.bat)'
}

$legacyTask = $null -ne (Get-ScheduledTask -TaskName 'LenovoIppPrinter' -ErrorAction SilentlyContinue)
if ($legacyTask) {
    Write-Host 'WARN : legacy scheduled task exists — run scripts/setup.ps1 to remove'
}

if (Test-Path -LiteralPath $script:IppPrinterVenvPython) {
    $zc = & $script:IppPrinterVenvPython -c "import zeroconf; print(zeroconf.__version__)" 2>&1
    if ($LASTEXITCODE -eq 0) {
        Write-Host "OK : zeroconf $zc"
    }
    else {
        Test-Line 'zeroconf' $false 'scripts/setup.ps1'
    }
}

Write-Host ''
if ($fail -eq 0) {
    Write-Host 'RESULT: OK'
    exit 0
}
Write-Host "RESULT: $fail issue(s) — run scripts/setup.ps1"
exit 1

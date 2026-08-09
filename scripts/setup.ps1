#Requires -Version 7.0
<#
.SYNOPSIS
  First-time setup for fake-printer: build the Rust exe, write .env, create dirs.
#>
param(
    [switch]$Force,
    [string]$InstallRoot = '',
    [string]$SpoolDir = '',
    [int]$RenderDpi = 200
)

. "$PSScriptRoot\lib\ipp-printer-lib.ps1"

if ($InstallRoot) {
    $script:IppPrinterInstallRoot = $InstallRoot
    $script:IppPrinterExe = Join-Path $InstallRoot 'fake-printer.exe'
    $script:IppPrinterManifest = Join-Path $InstallRoot 'install.json'
    $script:IppPrinterLogsDir = Join-Path $InstallRoot 'logs'
    $script:IppPrinterEnvFile = Join-Path $InstallRoot '.env'
}

$spool = if ($SpoolDir) { $SpoolDir } else { Join-Path $script:IppPrinterInstallRoot 'spool' }
$temp = Join-Path $script:IppPrinterInstallRoot 'temp'
$envTemplate = Join-Path $script:IppPrinterRepoRoot 'config\env.example'
$inboxDir = Join-Path $script:IppPrinterInstallRoot 'inbox'

Ensure-Dir $script:IppPrinterInstallRoot
Ensure-Dir $spool
Ensure-Dir $temp
Ensure-Dir $script:IppPrinterLogsDir
Ensure-Dir $inboxDir

Write-Host '=== fake-printer setup (Rust) ==='
Write-Host "root: $($script:IppPrinterInstallRoot)"

& "$PSScriptRoot\stop.ps1" 2>$null

if (-not (Test-CommandExists gswin64c) -and -not (Test-CommandExists gs)) {
    Write-Warning 'Ghostscript not found — install from https://ghostscript.com/releases/gsdnld.html (PostScript jobs only)'
}

# Build (or reuse) the release exe.
if ($Force -or -not (Test-Path -LiteralPath $script:IppPrinterReleaseExe)) {
    if (-not (Test-CommandExists cargo)) {
        throw 'cargo not found — install Rust from https://rustup.rs and retry'
    }
    Write-Host 'cargo: building release exe (first build downloads pdfium.dll)'
    Push-Location (Join-Path $script:IppPrinterRepoRoot 'rust')
    try {
        cargo build --release
        if ($LASTEXITCODE -ne 0) { throw "cargo build failed (exit $LASTEXITCODE)" }
    }
    finally {
        Pop-Location
    }
}
else {
    Write-Host "cargo: reusing $($script:IppPrinterReleaseExe) (use -Force to rebuild)"
}

if ($script:IppPrinterInstallRoot -ne $script:IppPrinterRepoRoot) {
    Copy-Item -LiteralPath $script:IppPrinterReleaseExe -Destination $script:IppPrinterExe -Force
    Write-Host "exe: copied -> $($script:IppPrinterExe)"
}
else {
    Copy-Item -LiteralPath $script:IppPrinterReleaseExe -Destination $script:IppPrinterExe -Force
    Write-Host "exe: staged -> $($script:IppPrinterExe)"
}

# .env lives at the install root (next to the exe).
# NOTE: values containing spaces must be quoted (KEY="a b") — see rust/README.md.
if ((Test-Path -LiteralPath $script:IppPrinterEnvFile) -and -not $Force) {
    Write-Host ".env: keeping existing $($script:IppPrinterEnvFile)"
}
else {
    $envContent = Get-Content -LiteralPath $envTemplate -Raw
    $envContent = $envContent.Replace('REPLACE_SPOOL_DIR', ($spool -replace '\\', '/'))
    $envContent = $envContent.Replace('REPLACE_TEMP_DIR', ($temp -replace '\\', '/'))
    $envContent = $envContent -replace 'IPP_RENDER_DPI=\d+', "IPP_RENDER_DPI=$RenderDpi"
    Set-Content -LiteralPath $script:IppPrinterEnvFile -Value $envContent -Encoding UTF8
    Write-Host ".env: written -> $($script:IppPrinterEnvFile)"
}
Write-Host "spool: $spool"

& "$PSScriptRoot\open-firewall.ps1"
Remove-LegacyScheduledTasks

$ippUrl = "ipp://$(Get-LocalIPv4):8631/ipp/print"
Remove-LegacyStartBat -InstallRoot $script:IppPrinterInstallRoot

Write-InstallManifest @{
    version      = 5
    deployed_at  = (Get-Date).ToString('o')
    install_root = $script:IppPrinterInstallRoot
    spool_dir    = $spool
    temp_dir     = $temp
    listen_port  = 8631
    render_dpi   = $RenderDpi
    local_ipv4   = (Get-LocalIPv4)
    ipp_url      = $ippUrl
    start_exe    = $script:IppPrinterExe
    inbox_dir    = $inboxDir
    launch_mode  = 'exe-foreground'
    engine       = 'rust'
}

Write-Host ''
Write-Host 'Setup complete.'
Write-Host "Start:   $($script:IppPrinterExe)"
Write-Host "IPP URL: $ippUrl"
Write-Host "Spool:   $spool"
Write-Host "Inbox:   $inboxDir"
Write-Host 'Next:    double-click fake-printer.exe (close window to stop)'

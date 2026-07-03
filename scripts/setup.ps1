#Requires -Version 7.0
<#
.SYNOPSIS
  First-time setup for fake-printer (venv, paperlessprinter, .env, dirs).
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
    $script:IppPrinterRepoDir = Join-Path $InstallRoot 'paperlessprinter'
    $script:IppPrinterVenvPython = Join-Path $InstallRoot '.venv\Scripts\python.exe'
    $script:IppPrinterManifest = Join-Path $InstallRoot 'install.json'
    $script:IppPrinterLogsDir = Join-Path $InstallRoot 'logs'
    $script:IppPrinterStartBat = Join-Path $InstallRoot 'start-fake-printer.bat'
}

$spool = if ($SpoolDir) { $SpoolDir } else { Join-Path $script:IppPrinterInstallRoot 'spool' }
$temp = Join-Path $script:IppPrinterInstallRoot 'temp'
$envFile = Join-Path $script:IppPrinterRepoDir '.env'
$envTemplate = Join-Path $script:IppPrinterRepoRoot 'config\env.example'
$inboxDir = Join-Path $script:IppPrinterInstallRoot 'inbox'

Ensure-Dir $script:IppPrinterInstallRoot
Ensure-Dir $spool
Ensure-Dir $temp
Ensure-Dir $script:IppPrinterLogsDir
Ensure-Dir $inboxDir

Write-Host '=== fake-printer setup ==='
Write-Host "root: $($script:IppPrinterInstallRoot)"

& "$PSScriptRoot\stop.ps1" 2>$null

Install-WingetPackage 'Python.Python.3.12' 'Python 3' | Out-Null
if (-not (Test-CommandExists gswin64c) -and -not (Test-CommandExists gs)) {
    Write-Warning 'Ghostscript not found — install from https://ghostscript.com/releases/gsdnld.html (PostScript jobs only)'
}

if (-not (Test-CommandExists python)) {
    throw 'python not found after winget install; open a new terminal and retry'
}

if ((Test-Path -LiteralPath $script:IppPrinterRepoDir) -and $Force) {
    Write-Host 'paperlessprinter: removing existing clone (-Force)'
    Remove-Item -LiteralPath $script:IppPrinterRepoDir -Recurse -Force
}

function Test-PaperlessPatchApplied([string]$PatchPath) {
    git -C $script:IppPrinterRepoDir apply --reverse --check $PatchPath 2>$null
    return ($LASTEXITCODE -eq 0)
}

function Undo-PaperlessPatches {
    $patchDir = Join-Path $script:IppPrinterRepoRoot 'patches'
    if (-not (Test-Path -LiteralPath $patchDir)) { return }
    foreach ($patch in Get-ChildItem -LiteralPath $patchDir -Filter '*.patch' | Sort-Object Name -Descending) {
        if (Test-PaperlessPatchApplied $patch.FullName) {
            git -C $script:IppPrinterRepoDir apply --reverse $patch.FullName
            Write-Host "patch: reverted for update - $($patch.Name)"
        }
    }
}

function Invoke-PaperlessPatches {
    $patchDir = Join-Path $script:IppPrinterRepoRoot 'patches'
    if (-not (Test-Path -LiteralPath $patchDir)) { return }
    foreach ($patch in Get-ChildItem -LiteralPath $patchDir -Filter '*.patch' | Sort-Object Name) {
        if (Test-PaperlessPatchApplied $patch.FullName) {
            Write-Host "patch: already applied - $($patch.Name)"
            continue
        }
        git -C $script:IppPrinterRepoDir apply --check $patch.FullName 2>$null
        if ($LASTEXITCODE -ne 0) {
            throw "patch: does not apply cleanly (upstream changed?) - $($patch.Name)"
        }
        git -C $script:IppPrinterRepoDir apply $patch.FullName
        Write-Host "patch: applied - $($patch.Name)"
    }
}

if (-not (Test-Path -LiteralPath $script:IppPrinterRepoDir)) {
    Write-Host "paperlessprinter: cloning -> $script:IppPrinterRepoDir"
    git clone --depth 1 $script:PaperlessRepoUrl $script:IppPrinterRepoDir
}
else {
    Write-Host 'paperlessprinter: updating'
    Undo-PaperlessPatches
    git -C $script:IppPrinterRepoDir pull --ff-only
}
Invoke-PaperlessPatches

if (-not (Test-Path -LiteralPath $script:IppPrinterVenvPython)) {
    Write-Host 'venv: creating'
    python -m venv (Join-Path $script:IppPrinterInstallRoot '.venv')
}

Write-Host 'venv: installing dependencies'
& $script:IppPrinterVenvPython -m pip install -q --upgrade pip
& $script:IppPrinterVenvPython -m pip install -q -r (Join-Path $script:IppPrinterRepoDir 'requirements.txt')
& $script:IppPrinterVenvPython -m pip install -q -r (Join-Path $script:IppPrinterRepoRoot 'requirements.txt')

$envContent = Get-Content -LiteralPath $envTemplate -Raw
$envContent = $envContent.Replace('REPLACE_SPOOL_DIR', ($spool -replace '\\', '/'))
$envContent = $envContent.Replace('REPLACE_TEMP_DIR', ($temp -replace '\\', '/'))
$envContent = $envContent -replace 'IPP_RENDER_DPI=\d+', "IPP_RENDER_DPI=$RenderDpi"
Set-Content -LiteralPath $envFile -Value $envContent -Encoding UTF8
Write-Host ".env: written -> $envFile"
Write-Host "spool: $spool"

& "$PSScriptRoot\open-firewall.ps1"
Remove-LegacyScheduledTasks

$ippUrl = "ipp://$(Get-LocalIPv4):8631/ipp/print"
Write-StartBat -InstallRoot $script:IppPrinterInstallRoot -IppUrl $ippUrl -SpoolDir $spool

Write-InstallManifest @{
    version        = 3
    deployed_at    = (Get-Date).ToString('o')
    install_root   = $script:IppPrinterInstallRoot
    spool_dir      = $spool
    temp_dir       = $temp
    listen_port    = 8631
    render_dpi     = $RenderDpi
    paperless_repo = $script:PaperlessRepoUrl
    local_ipv4     = (Get-LocalIPv4)
    ipp_url        = $ippUrl
    start_bat      = $script:IppPrinterStartBat
    inbox_dir      = $inboxDir
    launch_mode    = 'bat-foreground'
}

Write-Host ''
Write-Host 'Setup complete.'
Write-Host "Start:   $($script:IppPrinterStartBat)"
Write-Host "IPP URL: $ippUrl"
Write-Host "Spool:   $spool"
Write-Host "Inbox:   $inboxDir"
Write-Host 'Next:    double-click start-fake-printer.bat (close window to stop)'

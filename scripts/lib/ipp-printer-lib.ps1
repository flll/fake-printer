# Shared helpers for fake-printer scripts
$ErrorActionPreference = 'Stop'

# scripts/lib -> scripts -> repo root
$script:IppPrinterRepoRoot = Split-Path (Split-Path $PSScriptRoot -Parent) -Parent
$script:IppPrinterScriptsDir = Join-Path $script:IppPrinterRepoRoot 'scripts'
$script:IppPrinterTaskServer = 'FakePrinter'
$script:IppPrinterTaskMdns = 'FakePrinterMdns'
$script:PaperlessRepoUrl = 'https://github.com/paperlesspaper/paperlessprinter.git'

function Get-IppPrinterInstallRoot {
    param([string]$Override = '')

    if ($Override) {
        return $Override
    }

    $repoManifest = Join-Path $script:IppPrinterRepoRoot 'install.json'
    if (Test-Path -LiteralPath $repoManifest) {
        try {
            $data = Get-Content -LiteralPath $repoManifest -Raw | ConvertFrom-Json
            if ($data.install_root) {
                return [string]$data.install_root
            }
        }
        catch { }
        return $script:IppPrinterRepoRoot
    }

    # Legacy installs (migration)
    foreach ($legacy in @(
            'C:\OneDrive\fake-printer',
            (Join-Path $env:USERPROFILE '.cursor\lenovo-ipp-printer')
        )) {
        if (Test-Path -LiteralPath (Join-Path $legacy 'install.json')) {
            return $legacy
        }
    }

    return $script:IppPrinterRepoRoot
}

$script:IppPrinterInstallRoot = Get-IppPrinterInstallRoot
$script:IppPrinterRepoDir = Join-Path $script:IppPrinterInstallRoot 'paperlessprinter'
$script:IppPrinterVenvPython = Join-Path $script:IppPrinterInstallRoot '.venv\Scripts\python.exe'
$script:IppPrinterManifest = Join-Path $script:IppPrinterInstallRoot 'install.json'
$script:IppPrinterLogsDir = Join-Path $script:IppPrinterInstallRoot 'logs'
$script:IppPrinterStartBat = Join-Path $script:IppPrinterInstallRoot 'start-fake-printer.bat'

function Get-DefaultSpoolDir {
    return (Join-Path $script:IppPrinterInstallRoot 'spool')
}

function Ensure-Dir([string]$Path) {
    if (-not (Test-Path -LiteralPath $Path)) {
        New-Item -ItemType Directory -Force -Path $Path | Out-Null
    }
}

function Get-LocalIPv4 {
    try {
        $route = Get-NetRoute -DestinationPrefix '0.0.0.0/0' -ErrorAction Stop |
            Sort-Object -Property RouteMetric, ifMetric | Select-Object -First 1
        if ($route) {
            $srcIp = (Get-NetIPAddress -InterfaceIndex $route.ifIndex -AddressFamily IPv4 -ErrorAction Stop |
                Where-Object { $_.IPAddress -notmatch '^(127\.|169\.254\.)' } |
                Select-Object -First 1).IPAddress
            if ($srcIp) { return $srcIp }
        }
    }
    catch { }

    $addrs = Get-NetIPAddress -AddressFamily IPv4 -ErrorAction SilentlyContinue |
        Where-Object {
            $_.IPAddress -notmatch '^(127\.|169\.254\.|172\.(1[6-9]|2[0-9]|3[0-1])\.)' -and
            $_.PrefixOrigin -ne 'WellKnown'
        } |
        Sort-Object -Property InterfaceMetric, PrefixLength
    if ($addrs) {
        return $addrs[0].IPAddress
    }
    return '127.0.0.1'
}

function Test-CommandExists([string]$Name) {
    return [bool](Get-Command $Name -ErrorAction SilentlyContinue)
}

function Install-WingetPackage([string]$Id, [string]$Label) {
    if (-not (Test-CommandExists winget)) {
        Write-Warning "winget not found; install $Label manually"
        return $false
    }
    $list = winget list --id $Id --accept-source-agreements 2>$null | Out-String
    if ($list -match [regex]::Escape($Id)) {
        Write-Host "$Label : already installed ($Id)"
        return $true
    }
    Write-Host "$Label : installing via winget ($Id)..."
    winget install --id $Id --accept-package-agreements --accept-source-agreements --disable-interactivity 2>&1 | Out-Null
    if ($LASTEXITCODE -ne 0) {
        Write-Warning "winget install failed for $Id (exit $LASTEXITCODE); install manually if needed"
        return $false
    }
    return $true
}

function Read-InstallManifest {
    if (-not (Test-Path -LiteralPath $script:IppPrinterManifest)) {
        return $null
    }
    return Get-Content -LiteralPath $script:IppPrinterManifest -Raw | ConvertFrom-Json
}

function Write-InstallManifest([hashtable]$Data) {
    Ensure-Dir $script:IppPrinterInstallRoot
    ($Data | ConvertTo-Json -Depth 5) | Set-Content -LiteralPath $script:IppPrinterManifest -Encoding UTF8
}

function Remove-LegacyScheduledTasks {
    foreach ($taskName in @(
            $script:IppPrinterTaskServer,
            $script:IppPrinterTaskMdns,
            'LenovoIppPrinter',
            'LenovoIppPrinterMdns'
        )) {
        $task = Get-ScheduledTask -TaskName $taskName -ErrorAction SilentlyContinue
        if ($task) {
            Unregister-ScheduledTask -TaskName $taskName -Confirm:$false
            Write-Host "removed legacy scheduled task: $taskName"
        }
    }
}

function Write-StartBat([string]$InstallRoot, [string]$IppUrl, [string]$SpoolDir) {
    $batPath = Join-Path $InstallRoot 'start-fake-printer.bat'
    if ($InstallRoot -eq $script:IppPrinterRepoRoot) {
        Write-Host "start bat: $batPath (committed; no rewrite needed)"
        return
    }
    $lines = @(
        '@echo off'
        'title Fake Printer'
        'cd /d "%~dp0"'
        ''
        'set "PY=%~dp0.venv\Scripts\python.exe"'
        'if not exist "%PY%" ('
        '  echo [ERROR] Not set up. Run: pwsh scripts\setup.ps1'
        '  pause'
        '  exit /b 1'
        ')'
        ''
        '"%PY%" "%~dp0scripts\dashboard.py"'
    )
    Set-Content -LiteralPath $batPath -Value ($lines -join "`r`n") -Encoding ASCII
    Write-Host "start bat: $batPath"
}

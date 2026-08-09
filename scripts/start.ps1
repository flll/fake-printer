#Requires -Version 7.0
<#
.SYNOPSIS
  Launch fake-printer.exe in its own console window (close the window to stop).
#>
. "$PSScriptRoot\lib\ipp-printer-lib.ps1"

if (-not (Test-Path -LiteralPath $script:IppPrinterExe)) {
    throw 'fake-printer.exe not found; run scripts/setup.ps1 first'
}

Write-Host "launching: $script:IppPrinterExe"
Start-Process -FilePath $script:IppPrinterExe -WorkingDirectory $script:IppPrinterInstallRoot

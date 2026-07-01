#Requires -Version 7.0
<#
.SYNOPSIS
  Launch start-fake-printer.bat (foreground; close window to stop).
#>
. "$PSScriptRoot\lib\ipp-printer-lib.ps1"

if (-not (Test-Path -LiteralPath $script:IppPrinterStartBat)) {
    throw 'not set up; run scripts/setup.ps1 first'
}

Write-Host "launching: $script:IppPrinterStartBat"
Start-Process -FilePath 'cmd.exe' -ArgumentList "/c `"$script:IppPrinterStartBat`"" -WorkingDirectory $script:IppPrinterInstallRoot

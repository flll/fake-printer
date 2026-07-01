#Requires -Version 7.0
. "$PSScriptRoot\lib\ipp-printer-lib.ps1"

Write-Host '=== stop fake-printer ==='

Remove-LegacyScheduledTasks

function Stop-FakePrinterProcesses {
    Get-CimInstance Win32_Process -Filter "Name='python.exe'" -ErrorAction SilentlyContinue | ForEach-Object {
        $cmd = $_.CommandLine
        if ($null -eq $cmd) { return }
        if ($cmd -match 'server\.py' -or $cmd -match 'advertise-ipp-mdns\.py' -or $cmd -match 'dashboard\.py') {
            Write-Host "stopping PID $($_.ProcessId)"
            Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue
        }
    }
}

Stop-FakePrinterProcesses

$conns = Get-NetTCPConnection -LocalPort 8631 -State Listen -ErrorAction SilentlyContinue
if (-not $conns) {
    Write-Host 'OK: port 8631 is free'
}
else {
    Write-Warning 'port 8631 still in use'
}

#Requires -Version 7.0
. "$PSScriptRoot\lib\ipp-printer-lib.ps1"

$rules = @(
    @{ Name = 'FakePrinter-IPP-TCP'; Port = 8631; Protocol = 'TCP' }
    @{ Name = 'FakePrinter-mDNS-UDP'; Port = 5353; Protocol = 'UDP' }
)

foreach ($rule in $rules) {
    $existing = Get-NetFirewallRule -DisplayName $rule.Name -ErrorAction SilentlyContinue
    if ($existing) {
        Write-Host "firewall: $($rule.Name) already exists"
        continue
    }
    try {
        New-NetFirewallRule `
            -DisplayName $rule.Name `
            -Direction Inbound `
            -Action Allow `
            -Protocol $rule.Protocol `
            -LocalPort $rule.Port `
            -Profile Private, Domain `
            -ErrorAction Stop | Out-Null
        Write-Host "firewall: added $($rule.Name) ($($rule.Protocol) $($rule.Port))"
    }
    catch {
        Write-Warning "firewall: could not add $($rule.Name) (admin required). Run setup elevated or add rules manually."
    }
}

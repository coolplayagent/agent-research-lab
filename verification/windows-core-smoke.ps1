param([string]$Binary = (Join-Path $PSScriptRoot '..\target\debug\outcome-hub.exe'))
$ErrorActionPreference = 'Stop'
$binary = (Resolve-Path -LiteralPath $Binary).Path
$state = Join-Path $env:RUNNER_TEMP ('outcome-hub-' + [guid]::NewGuid().ToString('N'))
$reservation = [System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::Loopback, 0)
$reservation.Start()
$port = ([System.Net.IPEndPoint]$reservation.LocalEndpoint).Port
$reservation.Stop()
$listen = "127.0.0.1:$port"
$start = [System.Diagnostics.ProcessStartInfo]::new($binary)
$start.UseShellExecute = $false
foreach ($argument in @('serve', '--state-dir', $state, '--listen', $listen, '--max-seconds', '40')) {
    [void]$start.ArgumentList.Add($argument)
}
$server = [System.Diagnostics.Process]::Start($start)
try {
    $ready = $false
    for ($attempt = 0; $attempt -lt 30; $attempt++) {
        if ($server.HasExited) { throw "OutcomeHub exited early: $($server.ExitCode)" }
        try {
            $response = Invoke-WebRequest "http://$listen/" -UseBasicParsing -TimeoutSec 2
            if ($response.StatusCode -eq 200) { $ready = $true; break }
        } catch {
            Start-Sleep -Seconds 1
        }
    }
    if (-not $ready) { throw 'OutcomeHub did not answer on loopback.' }
    $session = Invoke-RestMethod "http://$listen/api/operator/session" -TimeoutSec 2
    if ($session.authenticated -ne $true -or $session.local_operator -ne $true) {
        throw 'The native workspace requires unexpected operator credentials.'
    }
    $link = & $binary link --state-dir $state
    if ($LASTEXITCODE -ne 0 -or $link -ne "http://$listen/") {
        throw 'The native core did not provide its direct loopback workspace URL.'
    }
    foreach ($path in @($state, (Join-Path $state 'dashboard'), (Join-Path $state 'private/managed-storage'))) {
        if ((Get-Acl -LiteralPath $path).Sddl -notmatch 'D:P') {
            throw "State directory has an inherited DACL: $path"
        }
    }
    Write-Output 'Windows core started, opened its local workspace directly and protected its state.'
} finally {
    if (-not $server.HasExited) { $server.Kill(); $server.WaitForExit(5000) | Out-Null }
}

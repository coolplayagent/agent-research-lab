param([Parameter(Mandatory)][string]$Binary)
$ErrorActionPreference = 'Stop'
$binary = (Resolve-Path -LiteralPath $Binary).Path
foreach ($name in @('outcome-hub-storage.exe', 'outcome-hub-knowledge.exe')) {
    if (-not (Test-Path -LiteralPath (Join-Path (Split-Path $binary) $name))) { throw "Package lacks $name" }
}
$root = Join-Path $env:RUNNER_TEMP ('knowledge-' + [guid]::NewGuid().ToString('N'))
$state = Join-Path $root 'state'
$notes = Join-Path $root 'external notes'
$repo = Join-Path $root 'external repo'
New-Item -ItemType Directory -Path $notes, $repo | Out-Null
[IO.File]::WriteAllText((Join-Path $notes 'note.md'), "directory source`n")
[IO.File]::WriteAllText((Join-Path $repo 'note.md'), "git source`n")
& git -C $repo init -b main
if ($LASTEXITCODE -ne 0) { throw 'Git init failed' }
& git -C $repo add .
& git -C $repo -c user.name=Fixture -c user.email=fixture@example.org -c commit.gpgSign=false commit -m baseline
if ($LASTEXITCODE -ne 0) { throw 'Git commit failed' }
& git -C $repo branch publish
$reservation = [Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback, 0)
$reservation.Start(); $port = ([Net.IPEndPoint]$reservation.LocalEndpoint).Port; $reservation.Stop()
$base = "http://127.0.0.1:$port"
function Start-HostProcess {
    $start = [Diagnostics.ProcessStartInfo]::new($binary); $start.UseShellExecute = $false
    foreach ($argument in @('serve', '--state-dir', $state, '--listen', "127.0.0.1:$port", '--max-seconds', '180')) { [void]$start.ArgumentList.Add($argument) }
    [Diagnostics.Process]::Start($start)
}
function Read-Api([string]$Path) { Invoke-RestMethod ($base + $Path) -TimeoutSec 20 }
function Write-Api([string]$Path, $Body) {
    Invoke-RestMethod ($base + $Path) -Method Post -ContentType 'application/json' -Headers @{Origin=$base; 'X-Crystal-Intent'='manage-knowledge'} -Body ($Body | ConvertTo-Json -Depth 25 -Compress) -TimeoutSec 40
}
function Wait-Ready {
    for ($attempt = 0; $attempt -lt 40; $attempt++) {
        if ($server.HasExited) { throw "Host exited: $($server.ExitCode)" }
        try { $session = Read-Api '/api/operator/session'; if ($session.authenticated) { return } } catch { Start-Sleep -Milliseconds 500 }
    }
    throw 'Host readiness deadline expired'
}
function Local($Request) {
    $view = Read-Api '/api/v1/collaboration/services'
    Write-Api '/api/v1/knowledge/local' @{configuration_revision=$view.configuration.revision; request=$Request}
}
function Proxy($Request) {
    $view = Read-Api '/api/v1/collaboration/services'
    Write-Api '/api/v1/knowledge' @{service_id=$service; configuration_revision=$view.configuration.revision; request=$Request}
}
$server = Start-HostProcess
try {
    Wait-Ready
    $initial = Read-Api '/api/v1/collaboration/services'
    if ($initial.configuration.outcome_services.Count -ne 0) { throw 'Knowledge started without demand' }
    $target = @{kind='directory'; path=$notes; content_root='.'; enabled=$true; access='read_only'; reference=$null; remote=$null; credential_env=@()}
    $added = Local @{operation='add'; name='notes'; target=$target}
    $service = $added.service_id
    if ((Proxy @{operation='read'; target='notes'; path='note.md'; revision=$null}).content -ne "directory source`n") { throw 'Directory read mismatch' }
    $target.kind = 'git'; $target.path = $repo
    $added = Local @{operation='add'; name='repository'; target=$target}
    if ($added.result.target.reference -ne 'refs/heads/main') { throw 'Git branch was not resolved' }
    if ((Proxy @{operation='read'; target='repository'; path='note.md'; revision=$null}).content -ne "git source`n") { throw 'Git read mismatch' }
    $target.access = 'read_write'; $target.reference = 'refs/heads/publish'
    Local @{operation='add'; name='publish'; target=$target} | Out-Null
    $saved = (Proxy @{operation='list'}) | ConvertTo-Json -Depth 25 -Compress
    $status = Read-Api '/api/v1/collaboration/services'
    if (($status.local_runtime | Where-Object component -eq 'knowledge').state -ne 'ready') { throw 'Knowledge health is not ready' }
    $server.Kill(); $server.WaitForExit(5000) | Out-Null
    $server = Start-HostProcess
    Wait-Ready
    if (((Proxy @{operation='list'}) | ConvertTo-Json -Depth 25 -Compress) -ne $saved) { throw 'Knowledge targets changed after restart' }
    if ((Proxy @{operation='read'; target='notes'; path='note.md'; revision=$null}).content -ne "directory source`n") { throw 'Restart lost directory target' }
    Write-Output 'Windows package: on-demand knowledge, directory/Git reads, prepared publication branch and restart passed.'
} finally {
    if (-not $server.HasExited) { $server.Kill(); $server.WaitForExit(5000) | Out-Null }
}

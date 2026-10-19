param(
    [Parameter(Mandatory)][string]$Binary,
    [string]$Fixture,
    [switch]$Live,
    [hashtable]$Programs = @{},
    [hashtable]$Models = @{},
    [string]$Report
)
$ErrorActionPreference = 'Stop'
if (-not $Live -and -not $Fixture) { throw 'Choose an explicit deterministic fixture or -Live.' }
if ($Live -and $Fixture) { throw 'Live acceptance cannot use a deterministic fixture.' }
$binary = (Resolve-Path -LiteralPath $Binary).Path
foreach ($name in @('outcome-hub-services.exe', 'outcome-hub-agent.exe')) {
    if (-not (Test-Path -LiteralPath (Join-Path (Split-Path $binary) $name))) { throw "Package lacks $name" }
}
$temporaryRoot = if ($env:RUNNER_TEMP) { $env:RUNNER_TEMP } else { [IO.Path]::GetTempPath() }
$state = Join-Path $temporaryRoot ('outcome-hub-binding-' + [guid]::NewGuid().ToString('N'))
$reservation = [Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback, 0)
$reservation.Start(); $port = ([Net.IPEndPoint]$reservation.LocalEndpoint).Port; $reservation.Stop()
$base = "http://127.0.0.1:$port"
$start = [Diagnostics.ProcessStartInfo]::new($binary); $start.UseShellExecute = $false
foreach ($argument in @('serve', '--state-dir', $state, '--listen', "127.0.0.1:$port", '--max-seconds', '600')) { [void]$start.ArgumentList.Add($argument) }
$server = [Diagnostics.Process]::Start($start)
function Read-Api([string]$Path) { Invoke-RestMethod ($base + $Path) -TimeoutSec 12 }
function Write-Api([string]$Path, $Body, [string]$Intent = 'manage-services') {
    Invoke-RestMethod ($base + $Path) -Method Post -ContentType 'application/json' -Headers @{Origin=$base; 'X-Crystal-Intent'=$Intent} -Body ($Body | ConvertTo-Json -Depth 25 -Compress) -TimeoutSec 12
}
function Wait-Binding([string]$Id) {
    $deadline = [DateTime]::UtcNow.AddSeconds(125); $sequence = -1
    do {
        $operation = Read-Api "/api/v1/collaboration/bindings/$Id"
        if ($operation.sequence -lt $sequence) { throw 'Operation sequence moved backwards.' }
        $sequence = $operation.sequence
        if ($operation.state -ne 'running') { return $operation }
        Start-Sleep -Milliseconds 250
    } while ([DateTime]::UtcNow -lt $deadline)
    throw 'Binding did not reach a terminal state within its bounded lifetime.'
}
$evidence = @()
try {
    $ready = $false
    for ($attempt = 0; $attempt -lt 40; $attempt++) {
        if ($server.HasExited) { throw "OutcomeHub exited early: $($server.ExitCode)" }
        try { $session = Read-Api '/api/operator/session'; if ($session.authenticated) { $ready = $true; break } } catch { Start-Sleep -Milliseconds 500 }
    }
    if (-not $ready) { throw 'Workspace did not become ready.' }
    $program = $null
    if ($Fixture) {
        $directory = Join-Path $state 'fixture programs'; New-Item -ItemType Directory -Path $directory | Out-Null
        $program = Join-Path $directory 'native-agent.exe'; Copy-Item -LiteralPath $Fixture -Destination $program
    }
    foreach ($agent in @('codex', 'cc', 'hds', 'pi')) {
        $person = "binding-$agent"
        Write-Api '/api/v1/messaging/manage' @{operation='person'; person=@{id=$person; name=$person}} 'manage-crystal' | Out-Null
        $view = Read-Api '/api/v1/collaboration/services'
        $model = if ($Models.ContainsKey($agent)) { $Models[$agent] } elseif ($Fixture -and $agent -eq 'hds') { 'fixture/deterministic' } else { $null }
        $selectedProgram = if ($Programs.ContainsKey($agent)) { $Programs[$agent] } else { $program }
        $request = @{request_id=[guid]::NewGuid().ToString(); person_id=$person; adapter_id=$agent; revision=$view.configuration.revision; program=$selectedProgram; model=$model}
        $operation = Write-Api '/api/v1/collaboration/bindings' $request
        $duplicate = Write-Api '/api/v1/collaboration/bindings' $request
        if ($duplicate.request.request_id -ne $operation.request.request_id) { throw 'Duplicate request created another operation.' }
        $result = Wait-Binding $request.request_id
        if ($result.state -ne 'succeeded') { throw "Binding $agent failed: $($result.diagnostic.code), stage $($result.diagnostic.stage), OS $($result.diagnostic.os_code), diagnostic $($result.diagnostic.diagnostic_id). State: $state" }
        if (@($result.steps | Where-Object state -ne 'passed').Count) { throw 'Successful binding has incomplete stages.' }
        $saved = Read-Api '/api/v1/collaboration/services'
        if ($saved.configuration.bindings.$person -ne $agent -or $saved.configuration.binding_evidence.$person.request_id -ne $request.request_id) { throw 'Binding and evidence were not saved together.' }
        $evidence += @{agent=$agent; state=$result.state; version=$result.program.version; evidence=$result.evidence; steps=$result.steps}
        Write-Output "Validated $agent (live=$Live), sandbox=$($result.evidence.sandbox_id)"
    }
    if ($Fixture) {
        # A real Node runtime and a hoisted dependency exercise npm layout in the sandbox.
        # This remains a deterministic protocol fixture, not authenticated vendor evidence.
        Get-Command node.exe -ErrorAction Stop | Out-Null
        $npm = Join-Path $state 'fixture npm'
        $package = Join-Path $npm 'node_modules/@openai/codex'
        $dependency = Join-Path $npm 'node_modules/binding-fixture-dependency'
        New-Item -ItemType Directory -Path (Join-Path $package 'bin'), $dependency -Force | Out-Null
        Copy-Item -LiteralPath $Fixture -Destination (Join-Path $package 'native-agent.exe')
        '{"name":"@openai/codex","bin":{"codex":"bin/codex.js"}}' | Set-Content -LiteralPath (Join-Path $package 'package.json') -Encoding utf8
        'module.exports = require("path").join(__dirname, "..", "@openai", "codex", "native-agent.exe");' | Set-Content -LiteralPath (Join-Path $dependency 'index.js') -Encoding utf8
        @'
const executable = require("binding-fixture-dependency");
require("child_process").spawn(executable, process.argv.slice(2), {stdio:"inherit", windowsHide:true})
  .on("error", () => process.exit(1)).on("exit", code => process.exit(code ?? 1));
'@ | Set-Content -LiteralPath (Join-Path $package 'bin/codex.js') -Encoding utf8
        $shim = Join-Path $npm 'codex.cmd'
        'exit /b 99' | Set-Content -LiteralPath $shim -Encoding ascii
        $saved = Read-Api '/api/v1/collaboration/services'
        $request = @{request_id=[guid]::NewGuid().ToString(); person_id='binding-codex'; adapter_id='codex'; revision=$saved.configuration.revision; program=$shim; model=$null}
        Write-Api '/api/v1/collaboration/bindings' $request | Out-Null
        $npmResult = Wait-Binding $request.request_id
        if ($npmResult.state -ne 'succeeded') { throw "npm dependency fixture failed: $($npmResult.diagnostic.code), stage $($npmResult.diagnostic.stage), OS $($npmResult.diagnostic.os_code), diagnostic $($npmResult.diagnostic.diagnostic_id). State: $state" }
        Write-Output 'Validated npm launcher with an installed Node runtime and hoisted dependency.'
        $saved = Read-Api '/api/v1/collaboration/services'
        $request = @{request_id=[guid]::NewGuid().ToString(); person_id='binding-codex'; adapter_id='codex'; revision=$saved.configuration.revision; program=(Join-Path $state 'missing.exe'); model=$null}
        Write-Api '/api/v1/collaboration/bindings' $request | Out-Null
        $failed = Wait-Binding $request.request_id
        if ($failed.state -ne 'failed' -or $failed.diagnostic.stage -ne 'discover') { throw 'Missing executable did not fail at discovery.' }
        $after = Read-Api '/api/v1/collaboration/services'
        if ($after.configuration.revision -ne $saved.configuration.revision) { throw 'Failed validation changed the configuration.' }
        $hang = Join-Path (Split-Path $program) 'hang-agent.exe'; Copy-Item -LiteralPath $Fixture -Destination $hang
        $request.request_id = [guid]::NewGuid().ToString(); $request.program = $hang
        Write-Api '/api/v1/collaboration/bindings' $request | Out-Null
        Start-Sleep -Seconds 2
        Write-Api "/api/v1/collaboration/bindings/$($request.request_id)/cancel" @{} | Out-Null
        $cancelled = Wait-Binding $request.request_id
        if ($cancelled.state -ne 'cancelled') { throw 'Cancellation did not terminate validation.' }
        Start-Sleep -Seconds 1
        if (Get-Process -Name 'hang-agent' -ErrorAction SilentlyContinue) { throw 'Cancelled agent process survived.' }
    }
    if ($Report) {
        @{live=[bool]$Live; binary_sha256=(Get-FileHash -LiteralPath $binary -Algorithm SHA256).Hash; windows=[Environment]::OSVersion.VersionString; agents=$evidence} | ConvertTo-Json -Depth 20 | Set-Content -LiteralPath $Report -Encoding utf8
    }
} finally {
    if (-not $server.HasExited) { $server.Kill(); $server.WaitForExit(5000) | Out-Null }
}

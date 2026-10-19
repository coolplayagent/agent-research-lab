# 10 Native Windows deployment

[Guides](00-index.md) · [简体中文](../../zh-CN/guide/10-windows.md)

Download `outcome-hub-windows-x86_64.zip` and `SHA256SUMS` from the same [GitHub release](https://github.com/coolplayagent/agent-research-lab/releases). Verify the archive in PowerShell before extraction:

```powershell
$expected = (Get-Content .\SHA256SUMS | Where-Object { $_ -match 'outcome-hub-windows-x86_64\.zip$' }).Split(' ')[0]
$actual = (Get-FileHash .\outcome-hub-windows-x86_64.zip -Algorithm SHA256).Hash.ToLowerInvariant()
if ($actual -ne $expected) { throw 'Archive checksum mismatch' }
Expand-Archive .\outcome-hub-windows-x86_64.zip -DestinationPath .\outcome-hub-release
```

Open two PowerShell windows in `outcome-hub-release\outcome-hub`. Start the native core in one and obtain its private browser link in the other:

```powershell
.\outcome-hub.exe serve --listen 127.0.0.1:4877 --max-seconds 43200
.\outcome-hub.exe link
```

The default state directory is `%LOCALAPPDATA%\OutcomeHub\state`; supply the same absolute `--state-dir` to both commands to use another location. The app creates private access-control lists for its state. Keep the link private and back up the state directory before upgrades. Stop the service before restoring a backup.

This native package runs core collaboration with its separate embedded local store. It does not include the external execution, sandbox, research, or knowledge services. The Windows defaults leave those services disabled; configured external service sockets cannot connect until a native service transport adapter is available. A configured adapter is not evidence of a completed task.

To build from source on Windows, install the pinned Rust toolchain and Bazelisk, then run `bazel build --lockfile_mode=error //:windows-package`. CI builds the archive on `windows-latest`, extracts it and smoke-tests the packaged executable.

# 10 Native Windows deployment

[Guides](00-index.md) · [简体中文](../../zh-CN/guide/10-windows.md)

Download `outcome-hub-windows-x86_64.zip` and `SHA256SUMS` from the same [GitHub release](https://github.com/coolplayagent/agent-research-lab/releases). Verify the archive in PowerShell before extraction:

```powershell
$expected = (Get-Content .\SHA256SUMS | Where-Object { $_ -match 'outcome-hub-windows-x86_64\.zip$' }).Split(' ')[0]
$actual = (Get-FileHash .\outcome-hub-windows-x86_64.zip -Algorithm SHA256).Hash.ToLowerInvariant()
if ($actual -ne $expected) { throw 'Archive checksum mismatch' }
Expand-Archive .\outcome-hub-windows-x86_64.zip -DestinationPath .\outcome-hub-release
```

Open PowerShell in `outcome-hub-release\outcome-hub` and start the native core:

```powershell
.\outcome-hub.exe serve --listen 127.0.0.1:4877 --max-seconds 43200
```

Open `http://127.0.0.1:4877/` directly in a browser on this computer. No management credential is needed. The default state directory is `%LOCALAPPDATA%\OutcomeHub\state`; use an absolute `--state-dir` to choose another location. The app creates private access-control lists for its state. Back up the state directory before upgrades and stop the service before restoring a backup.

The v0.2.1 package runs core collaboration with its separate embedded local store and includes local execution workers and the Agent bridge. Binding validation starts the managed execution and sandbox services when needed. Research and knowledge services remain optional external services. A configured adapter is not evidence of a completed task.

To build from source on Windows, install the pinned Rust toolchain and Bazelisk, then run `bazel build --lockfile_mode=error //:windows-package`. CI builds the archive on `windows-latest`, extracts it and smoke-tests the packaged executable.

## Native Agent bindings in v0.2.1

[v0.2.1](https://github.com/coolplayagent/agent-research-lab/releases/tag/v0.2.1) adds `outcome-hub-services.exe` and `outcome-hub-agent.exe`; keep them beside `outcome-hub.exe`. It fixes the Windows canonical-path handling that caused OS error 1 during binding. Automated Windows tests cover core startup, process isolation, four deterministic Agent protocol fixtures and installed Node launcher handling. For v0.2.1, the maintainer confirmed completion of authenticated Windows model acceptance for all four Agents on 2026-10-21. This is a maintainer attestation; private live-run reports are not included in the repository. Automated fixture tests establish protocol and isolation behavior within their tested scope.

Open Settings → Agent bindings, choose a digital person and Codex, Claude Code, DeepSeek Harness or Pi, then select **Validate and bind**. The app detects installed programs; it never installs third-party CLIs. A standalone `codex.exe` and an explicit absolute program path are supported. Leave the model blank to reuse the local selection. DeepSeek Harness needs an unambiguous configured route or an explicit `provider/model` override.

Binding runs one small model task and may incur a charge. Progress distinguishes program discovery, runtime checks, configuration, task execution and saving. You can cancel or reload the page without losing the operation. Validation times out after 120 seconds. A failure preserves the old binding; settings changes during validation require a fresh request. Existing bindings are shown as unverified until tested.

Use Windows 10/11 x64 with a local NTFS state directory. If Windows reports an error such as OS error 1, the UI includes its stage and a diagnostic ID. Check the state volume's permissions and filesystem support, the Agent's login and model settings, and the matching private diagnostic file. Do not publish credentials or private diagnostics.

Binding diagnostics are stored under `private/binding-<request-id>.diagnostic.json`. A failed native command may reference a `sandbox-...` identifier; its bounded stderr log is in `private/managed-sandbox/<identifier>.diagnostic.json`. These files stay on this computer and may contain sensitive Agent output.

For authenticated acceptance, run from a source checkout with all four CLIs installed and logged in:

```powershell
./verification/windows-binding-smoke.ps1 -Binary C:\OutcomeHub\outcome-hub.exe -Live -Report $env:TEMP\outcome-hub-live.json
```

Use `-Programs @{codex='C:\Tools\codex.exe'}` or `-Models @{hds='provider/model'}` for explicit overrides. The report is private evidence and belongs outside Git. CI's deterministic fixtures validate transport and isolation; they do not establish authenticated model compatibility.

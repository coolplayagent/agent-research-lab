OutcomeHub native Windows workspace
==============================

Extract the archive to a private directory. In PowerShell, run:

  .\outcome-hub.exe serve --listen 127.0.0.1:4877 --max-seconds 43200

Open http://127.0.0.1:4877/ directly in a browser on this computer;
no management credential is needed. The default state is under
%LOCALAPPDATA%\OutcomeHub\state. Use --state-dir to choose an alternate
private directory. Back up the state directory before upgrading.

This development package includes outcome-hub-services.exe and
outcome-hub-agent.exe. Keep these beside outcome-hub.exe. Native Agent support
is pending authenticated Windows acceptance and is not a released guarantee.

In Settings, select a digital person and an Agent, then choose Validate and bind.
OutcomeHub detects existing Codex, Claude Code, DeepSeek Harness and Pi programs;
it does not install them. A standalone codex.exe is supported. If detection
fails, enter an absolute program path. Existing login and model settings are
copied into disposable execution state. An optional model override is available;
DeepSeek Harness overrides use provider/model.

Binding runs one small real model task and may incur model charges. Progress
shows discovery, runtime, configuration, execution and persistence. Validation
has a 120-second bound and can be cancelled. Failed validation preserves the old
binding. Existing bindings remain marked unverified until validated.

Use a local NTFS state directory on Windows 10/11 x64. Isolation failures do not
fall back to unrestricted execution. Public errors show the failed stage,
OS error code and diagnostic ID; private diagnostic files are under the state
private directory. Research and knowledge services remain optional external
services.

Keep credentials and state out of Git. Verify the archive
against the SHA256SUMS file from the same GitHub release before use.

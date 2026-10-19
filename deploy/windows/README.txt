OutcomeHub native Windows core
==============================

Extract the archive to a private directory. In PowerShell, run:

  .\outcome-hub.exe serve --listen 127.0.0.1:4877 --max-seconds 43200

Open http://127.0.0.1:4877/ directly in a browser on this computer;
no management credential is needed. The default state is under
%LOCALAPPDATA%\OutcomeHub\state. Use --state-dir to choose an alternate
private directory. Back up the state directory before upgrading.

This package provides native core collaboration: identities, groups, messages,
goals and host review. External execution, research, sandbox and knowledge
services are not included in this Windows package. Their endpoints remain
unavailable until native service adapters are provided.

Keep credentials and state out of Git. Verify the archive
against the SHA256SUMS file from the same GitHub release before use.

OutcomeHub native Windows core
==============================

Extract the archive to a private directory. In PowerShell, run:

  .\outcome-hub.exe serve --listen 127.0.0.1:4877 --max-seconds 43200

In another PowerShell session, run:

  .\outcome-hub.exe link

Open the private link in a browser on this computer. The default state is under
%LOCALAPPDATA%\OutcomeHub\state. Use --state-dir with both commands to choose an
alternate private directory. Back up the state directory before upgrading.

This package provides native core collaboration: identities, groups, messages,
goals and host review. External execution, research, sandbox and knowledge
services are not included in this Windows package. Their endpoints remain
unavailable until native service adapters are provided.

Keep the private link, credentials and state out of Git. Verify the archive
against the SHA256SUMS file from the same GitHub release before use.

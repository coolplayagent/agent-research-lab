# Windows Agent binding implementation review

This is an implementation review by Codex, not an independent approval or a claim that four authenticated vendor CLIs have passed Windows acceptance. Release acceptance requires both the repository gates and `verification/windows-binding-smoke.ps1 -Live` on the packaged Windows binaries.

The collaboration host owns binding operations and the service configuration revision. It validates the person through the collaboration API before starting an operation. The executor and sandbox remain separate processes with separate private state. Neither opens the collaboration service's database. Core collaboration can still start without any coding Agent or execution service.

A binding request reserves a durable request identity before starting work. Duplicate requests return the same operation; a different payload cannot reuse its identity. At most four validations run concurrently, with one per person and a 120-second deadline. Cancellation drops the executor connection; connection loss propagates through the service chain to the sandbox's process guard. On Windows the process is created suspended, assigned to a kill-on-close Job Object, then resumed. The same startup discipline applies to discovery and vendor bridge subprocesses. Job memory and process-count limits are independent of the wall-time and output limits.

The binding operation has its own lifecycle in `contracts`, separate from goal attempts, worker claims and host acceptance. The frontend contract is generated from that source. Only a valid terminal vendor response with the current request identity is admitted. Pi prompt acknowledgement and `agent_end` are insufficient; the bridge requires settlement. DeepSeek Harness enqueue acknowledgement and idle notifications are insufficient; the bridge requires a completed turn, a correlated wait response and shutdown. Unknown terminal states fail validation.

The sandbox creates a fresh file challenge that is absent from the prompt. The task must read it and produce a separate file with exactly the expected bytes. The host checks the artifact after the process tree is stopped. A successful model statement alone cannot produce binding evidence. This is a capability check, not acceptance of any subsequent goal result.

Native program bytes, launcher entry bytes, adapter configuration, effective input and output are hashed. Native configuration snapshots are also pinned in the adapter command. A change to the program or configuration during validation prevents commit. Configuration and binding evidence are written together with revision compare-and-swap. A crash after that write is reconciled from the committed evidence; other interrupted operations require a new explicit request. Direct settings writes cannot create a new binding or forge evidence, and adapter edits invalidate stale evidence.

Windows uses private local named pipes with same-owner peer checks and remote clients rejected. AppContainer permissions apply to disposable copies of runtime files and configuration. Original installation and login directories are not modified by ACL grants. Runtime copies are read-only; the workspace and disposable home are writable. Credentials are copied only from known configuration locations or inherited by allowlisted environment names. They do not appear in command arguments, API results or public diagnostics. AppContainer profile markers allow cleanup after a service crash. There is no unrestricted execution fallback when isolation is unavailable.

The native health probe checks the child's Windows AppContainer token and workspace read/write access. Isolation regressions use native filesystem operations to prove host-private files remain inaccessible. npm installations retain their `node_modules` directory layout, including hoisted dependencies, inside the bounded read-only runtime copy. The Windows fixture suite separately exercises an installed Node runtime and a sibling dependency; the `.cmd` shim itself is never evaluated by a shell.

Node receives `--preserve-symlinks-main` and `--preserve-symlinks` for these link-free snapshots. This avoids module-loader realpath probes into inaccessible host ancestor directories while retaining the AppContainer ACL boundary. No permissions are granted to host volume roots or original installation directories.

Windows preparation and execution run outside the asynchronous request threads. Dropping a client operation signals cancellation to that worker; runtime copying, ACL traversal, filesystem-budget checks and process polling check cancellation and the same deadline. Copying is chunked and rejects files that change size. A cancellation before launch prevents the Agent process from starting, and cancellation during execution drops its Job Object before removing disposable state.

Failed native commands retain at most 64 KiB of stderr in a private sandbox diagnostic file, with a maximum of 32 retained failures. Public errors contain only its identifier. Treat these local files as sensitive; authenticated acceptance never prints their contents into CI logs. Deterministic fixture failures may print their own diagnostics to make platform regressions reproducible.

Verification is deliberately separated: deterministic native protocol peers test framing, binding persistence, process cleanup and failure handling without credentials; authenticated live acceptance tests vendor compatibility. The live script records binary hashes, versions, sandbox identity and validation evidence, and must be kept outside Git. Windows 10/11 x64 on a local NTFS state volume is the deployment target. OAuth arrangements that depend on an unavailable external helper, protected system credential stores, custom provider profiles, or a local proxy still need acceptance in the user's actual environment.

Protocol references checked during implementation:

- [Codex noninteractive mode](https://learn.chatgpt.com/docs/non-interactive-mode)
- [Claude Code programmatic CLI](https://code.claude.com/docs/en/headless)
- [Pi RPC lifecycle](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/rpc.md)
- [DeepSeek Harness SDK wire contract](https://github.com/deepseek-ai/deepseek-harness/blob/master/packages/sdk/protocol/src/types.ts)

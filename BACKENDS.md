# Local agent backends

The host scheduler owns task selection, workflow leases, attempts, cancellation,
wall-time budgets, task filesystem permissions and delivery authority. An execution
backend receives one bounded task. It cannot grant itself permission, enqueue
follow-ups directly, install tools globally, push or merge. Reports and proposed
follow-ups still pass the existing host validators and independent evaluation.

`codex` remains the default reserved backend. Existing `codex` and `[models]`
configuration, task prompts and serialized historical inputs retain their old
representation when the new fields are omitted. Codex uses `approval_policy=never`
and `sandbox_mode=danger-full-access` inside the existing host Bubblewrap boundary.
The named `kind = "codex"` variant selects another Codex executable; it does not
change that policy. Configure role bindings with the fragment in
[examples/backend.toml](examples/backend.toml).

## JSON process bridge v1

`kind = "json_process"` runs the configured absolute executable and literal `args`
without a shell. Supply a foreground executable wrapper, not a launcher for an
unmanaged daemon or remote bot. The wrapper may connect to a coding-agent service,
but must implement this contract and keep all local children under its lifetime.
No third-party SDK, provider or bot integration is claimed as tested here.

Capabilities are explicit host configuration: `structured_result` and
`read_workspace` are mandatory; writing requires `write_workspace`; required CLI
bindings require `tool_execution`; Computer Use additionally requires `desktop`.
Declarations permit admission; they are not evidence of actual compatibility.

The process receives exactly one UTF-8 JSON document on stdin, ending in a newline:

```json
{
  "schema_version": 1,
  "request_id": "task-id-attempt-1",
  "model": "backend-specific-model-id",
  "prompt": "The complete frozen task prompt",
  "response_schema": {"type": "object"},
  "worktree": "/absolute/private-state/worktrees/task-id",
  "result_path": "/absolute/private-state/runs/task-id-attempt-1/bridge-result.json",
  "bindings": {"source_commit": "...", "superpod_commit": "...", "prompt_digest": "...", "config_digest": "..."},
  "backend": {"id": "local-bot", "spec": {}, "executable": "/absolute/bridge", "executable_sha256": "..."},
  "permissions": {"worktree_write": false, "required_tools": [], "external_writes": false, "nested_agents": false, "authority": "host controller; backend declarations do not grant authority"},
  "timeout_seconds": 600
}
```

The example abbreviates the schema/spec and bindings; the actual request includes
the complete response schema and configured backend spec. Paths are absolute and
task-owned. Treat `timeout_seconds` as an upper bound; host preparation and global
budget deadlines can shorten it. The schema is also written to the task's
`result-schema.json` for inspection. Do not parse human-readable stdout as a result.

Hash the **exact stdin bytes**, including the final newline, with SHA-256. Write
one regular UTF-8 JSON file at `result_path`, then exit zero:

```json
{
  "schema_version": 1,
  "request_sha256": "64 lowercase hexadecimal characters",
  "report": {"summary": "...", "findings": [], "sources": [], "limitations": [], "next_tasks": []}
}
```

The result envelope permits only these three fields. `report` must satisfy the
provided task schema and host report validation. The result is limited to 1 MiB;
missing files, symlinks, FIFOs, oversized data, invalid JSON or an incorrect request
digest fail even when the process exits zero. The host freezes the request digest
in private launch authority **before** spawn; changing the writable request copy
cannot change that authority. A valid envelope establishes request association,
not truthfulness of claims. Stdout/stderr are diagnostics, each limited to 16 MiB.

The backend ID/spec/capabilities and resolved executable digest are frozen at
registration and rechecked before launch. This binds the executable bytes and
literal argument configuration, not transitively imported libraries, interpreter
modules, files named by arguments, remote model weights or provider behavior.
Use an immutable, versioned wrapper distribution and retain its independent
provenance when broader dependency coverage is required. Never put credentials in
`args` or model names: these are nonsecret configuration and enter private frozen
request/receipt metadata. Use explicitly allowed environment variables instead.

Generic adapters receive no Codex auth/config files, no `CODEX_HOME`, and no
OpenAI credentials by default. The host HOME is hidden; only public installed
skills and `.rustup` / `.cargo/bin` tooling are restored. Base locale/path/proxy
settings remain available. `env_allowlist` contains variable **names only**; values
are read at spawn and are not saved in request/config/receipt metadata. GH, Git,
SSH, loader-hook and isolation-path variable names are prohibited. Explicit model
API credential names are allowed. The same read-only root and task-specific
write mounts, PID namespace, heartbeat and process cancellation apply as for Codex.
This is a filesystem/process boundary, not network isolation or a guarantee that
a malicious network-capable backend respects external-service policy.

`doctor --models` runs a generic bridge in isolation with a tiny `{"ok":true}`
report schema. It reports a local JSON contract probe and leaves provider
compatibility unverified. Codex probes retain their model-response behavior.
`targets agent-check` currently requires Codex command-event evidence and rejects
a generic review backend explicitly; generic desktop tasks can use the owned
launcher when declared capable, but need backend-specific validation.

A future host communication module may add a read-only exported cohort directory
under `state/communication/views/<64-hex-id>`. The adapter hook rejects ancestor
paths and authority directories. Worker-writable output remains its own run logs;
a backend cannot choose another cohort's mount through its result.

## Validation and limits

Tests use explicitly fake local providers. The real workflow CLI and Bubblewrap
integration exercises success, candidate writes, malformed/missing/FIFO/oversized
results, request tampering, timeout, pause, and detached `setsid` descendants after
parent exit. The credential fixture checks synthetic HOME/AWS/Codex files and
inherited environment rejection. These validate protocol and host isolation;
they do not establish third-party coding quality or emergent collaboration.

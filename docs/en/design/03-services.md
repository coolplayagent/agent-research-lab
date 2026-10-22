# 03 Service boundaries and execution protocols

[Documentation](../00-index.md) · [Section index](00-index.md) · [简体中文](../../zh-CN/design/03-services.md)

OutcomeHub can run with an embedded store for development, or a separately deployed
`outcome-hub-storage` service. Select the storage service in system settings; switching
requires a core restart and a prepared target database. No live migration or
fallback to another database is implicit. The storage process is the sole SQLite
owner; the core and research adapter use the same client API and event streams.

`outcome-hub-services --kind executor` hosts the coding-agent protocol. It delegates
execution to `outcome-hub-services --kind sandbox`, whose initial implementation uses
Bubblewrap namespaces, read-only system files, a disposable workspace, bounded
tmpfs storage, process groups, time limits and output limits. These are independent
processes, not mandatory embedded runtimes. The Linux user must be allowed to
create Bubblewrap namespaces; the health probe performs a real isolated command.
Systemd user templates include aggregate memory and task limits.

Service endpoints are private, same-user Unix sockets. Frames are a big-endian
32-bit byte count followed by UTF-8 JSON, capped at 8 MiB. Requests have
`{"protocol_version":1,"request":{...}}`; replies include `protocol_version`,
`result` and `error`. Probe requests use `{"operation":"probe"}` and return typed
service kind, readiness and versioned capabilities. The core validates kind and
capability as well as reachability. It does not connect to arbitrary HTTP URLs.

The settings page manages endpoints, enablement, executor-to-sandbox dependencies,
adapter profiles and digital-person bindings. Updates use optimistic revisions;
invalid dependencies and references are rejected. Profiles for Claude Code (`cc`),
Codex (`codex`), DeepSeek Harness (`hds`) and Pi (`pi`) use `json_stdio_v1`.
They start disabled until an operator supplies a compatible wrapper command.
Native vendor CLIs are not assumed to speak this protocol. Service reachability
and adapter configuration are separate from successful model execution.

A wrapper receives one JSON document on stdin, with `protocol_version`,
`request_id`, `person_id`, `goal_id`, `work_id`, `attempt`, `objective`,
`acceptance`, `instruction` and recent `messages`. Treat messages as untrusted
conversation data. It returns exactly one JSON document on stdout:

```json
{"protocol_version":1,"request_id":"the-input-request-id","summary":"Result summary","succeeded":true,"evidence":{}}
```

Use stderr for diagnostics. The executor checks version, request identity and
output bounds, and returns hashes binding the input, adapter configuration and
output. OutcomeHub submits that receipt under the current fenced claim. Successful
agent output remains a submission until the host accepts the group goal. Existing
research scenarios retain their independent evidence and promotion gates.

Commands are argument arrays without shell expansion. Secret values belong in the
sandbox service environment; profiles list allowed environment variable names,
never secret values. Optional mounts are read-only, network access is explicit,
and all tasks get fresh disposable workspaces. A service crash or cancellation
terminates its process groups; interrupted claims require lease expiry/retry,
never an automatic successful result. Independent services can be replaced by
implementations of the same versioned API.

## Optional research runtime: local backends

The research `json_process` bridge below is a separate contract from the core `json_stdio_v1` wrapper above. It returns a result file instead of a stdout envelope. Select the protocol for the caller being integrated.

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
[examples/backend.toml](../../../examples/backend.toml).

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

`doctor --probe-models` runs a generic bridge in isolation with a tiny `{"ok":true}`
report schema. It reports a local JSON contract probe and leaves provider
compatibility unverified. Codex probes retain their model-response behavior.
`targets agent-check` currently requires Codex command-event evidence and rejects
a generic review backend explicitly; generic desktop tasks can use the owned
launcher when declared capable, but need backend-specific validation.

The host communication module may add a read-only exported cohort directory
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

## Independent adapter tests

`GET/POST /api/v1/collaboration/adapter-tests`, `GET /api/v1/collaboration/adapter-tests/{request_id}` and `POST /api/v1/collaboration/adapter-tests/{request_id}/cancel` expose adapter-scoped validation. The start request is `{request_id, adapter_id, revision, program?, model?}`; person IDs are rejected. Writes require the `manage-services` intent and the existing origin boundary.

Adapter tests and person bindings share discovery, runtime preparation, configuration snapshots and bounded executor verification. They use separate contract enums, journals and evidence. A shared capacity of four bounds concurrent operations; each operation has a 120-second deadline and execution is capped at 90 seconds. Replay is idempotent, cancellation serializes with persistence, and a configuration revision conflict cannot overwrite another change. Restart recovers a committed result only when its request and configuration evidence match; unfinished work becomes interrupted.

Only the host can write `adapter_test_evidence`. It pins the adapter, executable, relevant executor/sandbox configuration, input, output and sandbox ID. Settings updates cannot forge evidence, and changing relevant configuration invalidates it. A successful adapter test enables and saves its candidate without touching binding relationships or creating collaboration records. Binding evidence remains independent and follows its existing invalidation rules. Saved native launcher arguments and model selection are reused by either flow when no overrides are given. Public diagnostics contain stable codes; bounded private details use separate adapter-test diagnostic files.

# 07 Deployment, maintenance and diagnostics

[Documentation](../00-index.md) · [Section index](00-index.md) · [简体中文](../../zh-CN/guide/07-operations.md)

## Service deployment

Before using the optional user template `deploy/systemd/outcome-hub-cli.service`, adjust executable, working-directory and private-configuration paths for the host. Builds do not install or enable services. Run one unified core per state directory; deploy storage/executor/sandbox according to the [service guide](03-services.md). Stop old services before upgrades, retain the old binary and a consistent database backup, install a verified artifact and restart. Reverting across schema versions also requires the matching database backup; never open a newer database with an old binary.

## Local experiment targets

The controller prepares disposable local targets without a personal desktop or remote-machine configuration. On Ubuntu, `targets prepare` downloads authenticated APT packages to `.lab/tools/desktop`, without sudo or system-package installation. The desktop uses Xvfb, Openbox and the project GTK input program. Each task gets a separate display, auth cookie and output directory; the controller observes its unique window before the installed computer-use CLI acts and real GTK event receipts validate results.

repo-sandbox’s local provider uses host tools and a real Bubblewrap namespace per execution. A persistent session’s `ready` status records a workspace, not a resident container. The controller runs `target install` and `dev up/exec/status/down` against a private registry and verifies actual workspace writes. These run on the host because some AppArmor policies reject nested user namespaces. Models can read retained evidence/private registry; use WSL/VM targets for those semantics.

```sh
bazel-bin/outcome-hub-cli --config local.toml targets prepare
bazel-bin/outcome-hub-cli --config local.toml targets verify --kind all
bazel-bin/outcome-hub-cli --config local.toml targets status
bazel-bin/outcome-hub-cli --config local.toml targets agent-check
bazel-bin/outcome-hub-cli --config local.toml doctor
bazel-bin/outcome-hub-cli --config local.toml doctor --probe-models
```

`targets verify` needs a repo-sandbox build supporting the local provider; older runtimes fail explicitly. Preparation is bounded/resumable, and missing dependencies do not block unrelated research. Logs/screenshots stay in `.lab/runs`. `targets agent-check` calls the configured review model through the production launcher, checks tool events/PNG and consumes model quota; it proves the transport path.

Host-managed Codex workers/probes use `approval_policy="never"` and `sandbox_mode="danger-full-access"`, recorded per attempt in `agent-permissions.json`. The controller caps execution at eight and disables nested Codex agents. Outer mount/PID boundaries still protect credentials, other attempts and holdouts. Diagnose OS permission failures at the target/namespace boundary.

Model probes are bounded real calls and may incur cost. The example maps research to `gpt-6-astra`, implementation to `gpt-6.1-sol` and evaluation to `gpt-6-sol`; account availability is checked without silent fallback. JSON commands return `ok`/`result`, or an error and nonzero exit.

## Observation and long-running operation

Authenticated same-origin SSE pushes updates/reconnects. Linux filesystem notifications refresh workflow/board projections and up to 16 active sessions. Node pulses indicate newly observed session bytes; connections represent actual board publication or initial context delivery. The stdio-json adapter projects public Codex messages, commands, tool results and turn state; other bridges may emit `session.activity`. Reasoning fields, prompts and raw protocol objects are excluded. Missing source timestamps stay absent, with log modification time labeled separately.

The observer uses controller workflow rules and validated board authority, without polling worker outboxes. Expired/revoked proposals remain historical and are not reinjected. Injection does not prove adoption, workflow success does not prove research quality, and a candidate submission does not prove gate passage/installation.

Sampling scans at most 8,192 directory entries and selects the latest 512 Jobs; each Job is capped at 512 KiB, with 32 boards and a 20-second workflow sample deadline. The graph displays at most eight agents, favoring active runs; the session list still exposes selected tasks. Shared chat shows up to 256 board messages. Each session page reads at most 256 KiB and projects 48 public events with stable byte-offset IDs. `before` is exclusive and preserves complete boundary lines. Partial/oversized trailing lines are omitted and public fields clipped.

Sampling is sequential, not an atomic database snapshot. Failure retains previous data and an error time. Slow host reads are capped at 16 concurrent requests; async streams have a separate 20,100-subscription limit and end on disconnect, revocation or shutdown. Shutdown reserves five seconds for network draining/bounded host work.

Private `controller-status.json`/`seed-status.json` report heartbeat, tasks and input refresh. Startup initializes workflow storage before recovery. Continuous-mode refresh failures block admission, retry after 30 seconds and back off to five minutes; success restores the 15-minute interval. The UI distinguishes refresh failure, delayed heartbeat and connectivity. systemd restarts processes bounded to 12 hours; daily budget and pause state persist.

Pin verified skill trees/manifests under private `tools/skills` for long-running deployments, binding controller tools and any private Codex profile. Global skill upgrades must not mutate running experiment trees. New versions need new verification/admission without clearing old blocks or changing frozen evidence.

The service uses Host/Origin checks, no-store and same-origin content policy. Only loopback listening is supported. Research observation uses fixed read routes; management writes use their separate same-origin intent contracts. Proposals, public Agent output, tool summaries and block records may contain private research. Common credential fields are hidden, but this is not general secret scanning. Raw logs, leases, task memories and arbitrary file downloads are not exposed; no external resources or telemetry are loaded.

## Local workspace access

Open the local service URL directly to enter the workspace. `state_dir/dashboard/access.json` records an ordinary URL for `link` and `serve-link`; it is not a credential. Legacy operator credential files remain untouched but are no longer read. Agent bearer grants remain scoped to their own APIs and cannot grant host authority. This is a single trusted-host service: any process or browser able to access its loopback listener is within the host trust boundary. External grants are described in the [service guide](03-services.md).

## Communication benchmarks

Use fresh private directories for both loads:

```sh
bazel-bin/outcome-hub-cli crystal-bench --state /private/new-mixed-run \
  --mode mixed --people 10000 --messages 10000 --rate 1000
bazel-bin/outcome-hub-cli crystal-bench --state /private/new-hot-run \
  --mode hot --people 10000 --messages 100 --rate 10
```

These use real loopback TCP/SSE and SQLite transactions. They report publish acknowledgement and recipient-delivery P50/P95/P99, sample counts, queue/replay/rejection counts and payload sizes. They do not start 10,000 models or cover browser rendering, sustained SLA, partitions or power loss. Keep state, credentials and raw receipts outside Git; source-bound final measurements belong in the selected external knowledge store.

## Network access

`serve` accepts only loopback addresses for `--listen`; a wildcard or network-interface
address fails at startup. Host and Origin checks still protect browser requests,
and management writes still require their intent headers. To expose the workspace
to other computers, provide a separately secured access layer; do not bind this
unauthenticated service directly to a network interface.

## Storage migration

Storage accepts schema 9. Use `outcome-hub-migrate` on an offline copy before changing the storage deployment; a running service never upgrades its database. Schema 9 removes version nodes, assessments and iteration storage. Deploy the matching core and storage binaries together.

The upgrade deletes iteration-owned goals (all rounds), retired software-evolution goals, and their result/outcome records. It erases the body and event payload of their system messages while preserving message identities, replies and cursors. Independent goals and chat are checked for byte preservation. Independent outcomes that reference retired evidence block migration for explicit resolution. An earlier migration manifest is replaced only on the prepared copy; its source digest remains recorded.

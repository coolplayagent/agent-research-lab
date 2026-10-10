# Agent Research Lab

A Rust + Bazel controller for AI-SDLC and long-horizon agent research. It runs
different Codex models in isolated task worktrees, preserves experimental
bindings, evaluates prompt variants, and provides durable delivery adapters for
CLI installations, GitHub PRs and research publication.

Execution backends are modular: Codex remains the default, and a versioned local
JSON process bridge can connect an independently implemented coding agent or bot.
Role bindings, capability checks, credential isolation and the request/result
contract are documented in [BACKENDS.md](BACKENDS.md). External provider
compatibility must be tested separately; host scheduling and permissions remain
unchanged by backend selection.

[Multi-agent coordination](MULTI_AGENT.md) adds host-bound shared proposal boards,
topic-based context selection, quotas and recovery without message-triggered task
creation. Normal execution remains capped at eight workers. The included 1,000 /
10,000 participant simulations test bounded cells and pull-based summary routing;
they do not start thousands of model processes or claim research-quality gains.

Research knowledge belongs to the existing private
[SuperPOD repository](https://github.com/stevetdp/superpod). Access to this public
project does not grant access to SuperPOD. This repository contains the research
software, prompts and synthetic test fixtures; it does not copy private knowledge,
credentials or experiment traces into Git.

Reviewed, publicly shareable lessons and hypotheses live in
[research/insights.md](research/insights.md). New tasks automatically receive that
file from the project's current remote default-branch commit, with a frozen content
digest. Each entry retains evidence, version limits, counterexamples and a next
experiment. Detailed knowledge and source archives continue to live in SuperPOD.

## Build and configure

The repository is a Cargo workspace with an independent crate for each module:

```text
app/                         # CLI and application service composition
  src/{main,cli,service}.rs
  systemd/                   # Optional user-service template
crates/
  config/                    # Configuration and backend/team declarations
  task/                      # Frozen task, job and launch records
  runtime/                   # Durable scheduling and recovery
  agent-backend/              # Coding-agent invocation and result validation
  multi-agent/               # Host coordination and scale simulations
  communication/             # Bounded shared proposal boards
  ...                        # One Cargo.toml and BUILD.bazel per module
Cargo.toml                   # Workspace members and shared dependency versions
BUILD.bazel                  # Stable binary, aggregate tests and package targets
```

See [ARCHITECTURE.md](ARCHITECTURE.md) for module ownership and dependency rules.
`app/` composes the libraries; library crates do not depend on the application.
The application package and executable remain named `agent-research-lab`.

Linux is the first supported runtime platform. Install Git, Bazelisk/Bazel,
Bubblewrap (`bwrap` with working user namespaces), Codex CLI and GitHub CLI (`gh`).
The pinned Bazel and Rust versions are recorded in `.bazelversion` and
`rust-toolchain.toml`. Bazel uses `rules_rust`; Cargo manifests and lockfiles remain
the Rust dependency source. A working Rust toolchain is also needed for formatting
and Clippy checks.

```sh
bazel build -c opt --lockfile_mode=error //:agent-research-lab //:package
bazel test -c opt --lockfile_mode=error //...
cp examples/lab.toml local.toml
```

Use the optimized build for sustained operation: unoptimized hashing of the full
installed skill trees can consume a substantial part of the host deadline.

Edit `local.toml` before running. Its paths are examples from the development
machine: set your workspace, private runtime directory, existing SuperPOD checkout,
Codex executable, published workflow CLI and the seven installed tool bindings.
Repository paths must refer to existing local Git checkouts. Configure model/API
authentication through your installed tools; do not put credentials into the
tracked example configuration.

Production defaults to `require_latest = true`. Set `skills_manifest` to a host-owned
absolute JSON path recording all seven installed skill directories, runtime paths,
tree/runtime SHA-256 digests, and latest release tag/ID (or the latest default-branch
source commit). If a stable release exists, installing a newer local source build
also requires `source_qualitygate: {path, sha256}`: an immutable full, nonempty
passing diff report at that exact source commit. Retain build provenance and label
the installation as a development build; it is not a published release. Without
this evidence the existing release requirement remains in force.
The Rust types `freshness::SkillsManifest` and
`SkillSnapshot` define the exact schema; `freshness::skill_tree_digest` defines the
directory hash. Populate this evidence only after installing and verifying the
actual package. Five tools provide upstream skills; the two explicitly project-owned
adapters in `skills/` document Computer Use and Relay Memory where upstream provides
no skill. Runtime binaries are bundled during local installation, not tracked here.

Use `agent-research-lab skill-digest /absolute/path/to/installed-skill` to calculate
the canonical directory digest; retain release/download verification separately.

The controller checks GitHub's current stable release identity and installed bytes,
fetches each remote default branch, and builds separate readonly source worktrees.
It does not switch or overwrite a developer's branch. Latest release and latest
source are recorded separately: reproduce a proposed source defect on a build of
the pinned current source before treating an older released binary's failure as
evidence. Missing network access, changed skills or an advanced upstream blocks
admission; there is no fallback to an old local checkout. Install and validate a
new upstream package and refresh its host manifest when a release changes.

```sh
bazel-bin/agent-research-lab --config local.toml refresh
```

Independent skill and repository checks run in groups of at most four under the
same absolute host deadline. All started checks finish before an error returns;
a partial collection never replaces the current input manifest.

Each idle admission batch verifies freshness before starting up to eight agents of
one cohort. An independent heartbeat renews each active workflow lease while
the controller prepares other agents or handles results. The batch keeps
its source snapshots fixed. Installed skill/runtime bytes are checked again at
launch and settlement; concurrent installation changes invalidate the result.
Source, skill, configuration or research-input changes produce
new daily task IDs; old experiments remain available as historical evidence.
When idle, continuous mode checks for a new cohort every 15 minutes. Explicit
`require_latest = false` is intended only for offline synthetic fixtures and does
not establish current-version research evidence.

| Binding | Purpose |
| --- | --- |
| `workflow-cli` | Durable job state, claims, leases and evidence settlement |
| `relay-knowledge` | SuperPOD map validation, routing and exact-commit indexing |
| `into-markdown` | Source material conversion in research tasks |
| `qualitygate-cli` | Full, snapshot-bound release verification |
| `computer-use-cli` | Tasks against a dedicated test desktop/browser session |
| `relay-memory` | Isolated task memory, with explicit local storage |
| `repo-sandbox` | Tasks against configured execution environments |

The controller can prepare disposable local targets without personal desktop or
remote-machine configuration. On Ubuntu, `targets prepare` downloads authenticated
APT packages into `.lab/tools/desktop`; it needs no sudo and does not install system
packages. The real desktop uses Xvfb, Openbox and an owned GTK input application.
Every computer research task gets a separate display, authority cookie and output
directory. The controller first observes its unique window; agents can then use
the installed computer-use CLI and compare inputs with actual GTK event receipts.

The repo-sandbox local provider uses host tools and creates a real Bubblewrap
namespace for each execution. Its durable session records a workspace and reports
`ready`; it does not represent a running Docker container or persistent service.
The controller runs `target install`, `dev up/exec/status/down` with a private
registry and verifies an actual workspace write. This happens on the host because
some AppArmor policies forbid nested user namespaces inside an agent. Models get
the retained evidence and can inspect the private registry. WSL/VM targets remain
available through repo-sandbox for experiments needing those semantics.

```sh
bazel-bin/agent-research-lab --config local.toml targets prepare
bazel-bin/agent-research-lab --config local.toml targets verify --kind all
bazel-bin/agent-research-lab --config local.toml targets status
bazel-bin/agent-research-lab --config local.toml targets agent-check
```

`targets verify` requires the local-provider repo-sandbox build; an older runtime
fails visibly instead of treating help output as readiness. Preparation is bounded
and resumable. Missing prerequisites produce actionable errors while unrelated
research continues. Real verification logs and screenshots stay under `.lab/runs`.
`targets agent-check` makes a bounded live call with the configured review model
through the production target launcher and checks actual tool events and a PNG.
It consumes provider usage and verifies transport, not independent model quality.

All controller-owned Codex workers and model probes explicitly use
`approval_policy = "never"` and `sandbox_mode = "danger-full-access"` (all approved).
Each attempt records these arguments in `agent-permissions.json`. The controller
owns the eight-agent limit and disables nested Codex agents. The outer mount/PID
boundary still keeps host credentials, other attempts and evaluation holdouts
private. OS permission errors need target or namespace diagnosis; an approval flag
cannot grant missing kernel privileges.

```sh
bazel-bin/agent-research-lab --config local.toml doctor
bazel-bin/agent-research-lab --config local.toml doctor --probe-models
```

Model probes make bounded live calls and consume provider usage. The example
assigns `gpt-6-astra` to research, `gpt-6.1-sol` to implementation and `gpt-6-sol`
to review. Availability is account-dependent; unavailable models are reported
without silently substituting another model. JSON commands return an `ok` flag
and `result`, or an error with a nonzero process exit status.

## Run a bounded research cycle

```sh
bazel-bin/agent-research-lab --config local.toml enqueue examples/task.json
bazel-bin/agent-research-lab --config local.toml run --max-seconds 900
bazel-bin/agent-research-lab --config local.toml status
bazel-bin/agent-research-lab --config local.toml report
```

`enqueue` binds a task ID to its inputs, source commit, SuperPOD commit, prompt and
configuration. Reusing an ID with different inputs is rejected. Each task gets an
isolated worktree; dependencies receive validated completed reports. `write: false`
is the default. `required_tools` declares admission prerequisites (also supported on follow-up proposals); `use_memory`
enables task-local relay-memory. Memory retains a bounded result checkpoint and
immutable receipt references; full source/skill snapshots stay in the private
receipt. An explicit `prompt_version` selects a registered
prompt instead of the built-in role prompt.

`seed` adds a daily set of independent research, criticism and synthesis tasks
covering delivery, memory/recovery, collaboration and computer/sandbox operation.
It can also be requested with `run --seed`. Synthesis may propose bounded
follow-up tasks in allowlisted repositories; implementation may request a read-only
review. The controller limits proposal count and depth. Agent proposals do not
authorize installations, remote writes, publications or merges.

```sh
bazel-bin/agent-research-lab --config local.toml pause
bazel-bin/agent-research-lab --config local.toml resume
bazel-bin/agent-research-lab --config local.toml run --max-seconds 900
```

`pause` requests admission stop and child cancellation; `resume` clears that flag.
Restarting `run` reconciles persisted attempts and actual process identities.
Read-only failures have bounded retries; `retry TASK_ID` explicitly retries an
eligible read-only task. Interrupted writes retain their uncertain outcome for
reconciliation instead of being automatically repeated. Resume does not guarantee
instruction-level continuation inside a previously terminated model process.
Retained task starts replay their original timeout and input envelope, even when
the next controller uses a different timeout default. Successful workflow receipts
reconstruct durable completion work after a crash. Follow-up admission is replayable;
interrupted memory writeback remains explicitly unknown until reconciled, avoiding
an unproven duplicate write. Completion work, freshness checks and seeding share
the persisted daily budget and the `run --max-seconds` deadline.

The default ceiling is eight concurrent agents and 43,200 active wall-clock seconds
per Asia/Singapore calendar day. Concurrent agents share the wall-clock allowance;
it is not a dollar or token cap. Budget state survives controller restarts. Run the
bounded cycle and inspect its results before starting a service:

```sh
bazel-bin/agent-research-lab --config local.toml run --continuous --seed
```

`app/systemd/agent-research-lab.service` is an optional systemd user-service template.
Adjust its executable, working directory and configuration paths to your machine;
installation or service activation is not performed by building the project.

## Prompt and collaboration experiments

Strategies retain immutable parentage, role, task family, change reason and failure
conditions. Registration does not automatically activate a prompt or promote it.
Use an explicit version in a later task after evaluating its suitability.

```sh
mkdir -p .lab/evolution
bazel-bin/agent-research-lab evolution prompt-register \
  examples/evolution/prompt-register.json --output .lab/evolution/prompts.json
```

The runtime looks for its registry at `state_dir/evolution/prompts.json`; adjust
the output path if your configured state directory differs. Other actions are
`prompt-portfolio`, `evaluate`, `novelty`, `exploration`, `rule-proposal` and
`holdout-validate`. Their typed JSON examples and exact path conventions are in
[examples/evolution](examples/evolution/README.md).

The gate computes decisions from separately pinned host context and observations.
It rejects unknown fields, stale bindings, missing measurements and critical
regressions. A task's successful process exit or persuasive report is not an
independent evaluation. The host must supply measurements from actual evaluator
runs; the shipped observation files are synthetic fixtures, not scientific results.

Prompt promotion requires coverage across three task families and repeated paired
trials. The gate compares success rate and measured cost; novel-capability claims
also need held-out transfer and ablation evidence. Portfolios retain one stable,
two specialized and two exploratory strategies per task kind. Exploration starts
at 25%, adjusts after complete ten-experiment windows, and stays within 10–50%.
Formal rule proposals remain quarantined and cannot approve themselves.

Keep held-out tasks under `state_dir/private`; do not place them in SuperPOD,
shared memory or optimizer worktrees. Input/context/receipt files for formal gates
must be host-owned and outside candidate writable mounts. Merely storing files in
different directories does not create a security boundary.

## Host delivery outbox

The host can enqueue explicit, reviewed operations in `state_dir/outbox`:

```sh
bazel-bin/agent-research-lab --config local.toml automation enqueue --input local-operation.json
bazel-bin/agent-research-lab --config local.toml automation tick
```

An operation has the JSON envelope `{"kind":"…","request":{…}}`.

| Kind | Request type | Effect |
| --- | --- | --- |
| `install` | `delivery::InstallRequest` | Stage and smoke-test a verified skill plus bundled CLI |
| `pr` | `delivery::PullRequestRequest` | Reuse or create a PR for an already-pushed exact head |
| `merge` | `delivery::MergeRequest` | Verify evidence and live GitHub gates before exact-head merge |
| `knowledge_prepare` | `knowledge::PrepareReportRequest` | Prepare an isolated SuperPOD contribution worktree |
| `knowledge_publish` | `knowledge::PublishReportRequest` | Commit, push and open the reviewed SuperPOD PR |
| `knowledge_refresh` | `knowledge::IndexRefreshRequest` | Update/poll the exact SuperPOD index target |

These public Rust types in `crates/delivery/src/lib.rs` and
`crates/knowledge/src/lib.rs` are the JSON
request contracts. The queue stores immutable hashed requests and durable results.
Identical requests reuse their existing state. Index tasks can remain `waiting`;
unknown write outcomes become `needs_reconciliation`. After inspecting actual
remote, worktree or installed state, the host can use
`automation reconcile-retry --id OPERATION_DIGEST`. A retry is not permission to
ignore failed release gates.

Release operations require hashed files under `outbox/evidence`: the actual full
Qualitygate report, a host snapshot receipt binding that report to code, prompt,
policy and SuperPOD, and a distinct evaluator's result receipt. The installation
snapshot also binds the package and binary hashes. Boolean pass flags alone are
insufficient. Current upstream-default bindings and a hashed skill manifest are
also required, and promotion/release gates recheck them online immediately before
the effect. Old receipts without these bindings cannot authorize new releases.
GitHub's expected-head precondition does not atomically freeze the base branch;
strict up-to-date branch protection or a merge queue is needed for that server-side
race guarantee. These receipts must come from real checks and recorded invocations;
do not manufacture them to make a request pass.

An installation packages the binary inside the skill directory and keeps previous
versions outside skill discovery. The active binary is smoke-tested; failures
restore the prior version. Later edits prevent an old receipt from overwriting a
newer installation. GitHub delivery verifies current CI, mergeability, required
review decisions and unresolved threads. It does not use administrator bypasses,
invent reviewer approval, or claim a queued merge is already merged.

Direct host commands are also available: `install INPUT`, `rollback RECEIPT`,
`pr INPUT`, `merge INPUT`, `knowledge-prepare INPUT`, `knowledge-publish INPUT` and
`knowledge-refresh INPUT`. They are explicit effectful operations, not agent
report-parsing shortcuts. The controller does not convert arbitrary model output
into release authority.

## SuperPOD publication and evidence boundaries

With `require_latest = true`, every role receives the same bounded SuperPOD
entrypoint pack: `knowledge/index.md`, the AI-SDLC index, and the evidence-method
document. The host reads committed regular Git blobs and records the SuperPOD
commit, path, complete source digest, excerpt digest and truncation status. Each
excerpt is at most 16 KiB; full source files must be nonempty UTF-8 and at most
1 MiB. Admission checks the pack against the pinned commit. Agents follow
task-relevant links in that read-only snapshot. Context delivery alone does not
prove that a model read the linked material or that its conclusion is correct.

All roles propose useful additions, corrections, counterexamples and next tests
in their durable result receipts, with an existing SuperPOD destination and
baseline. The host reviews evidence, prepares an isolated contribution, merges
reviewed updates serially and refreshes the exact merged index for later cohorts.
These explicit host adapters remain necessary; a model proposal or successful
task does not itself publish knowledge. `relay-memory` retains isolated task
continuity; SuperPOD remains the shared, versioned knowledge base. Historical
receipts without the pack keep their original prompt and cohort identities.

`knowledge-prepare` renders a typed, reviewed research report under
`knowledge/software/ai-sdlc/` in a dedicated SuperPOD worktree. It keeps source IDs,
citations, retrieval status, available checksums and immutable experiment bindings
together. Measured claims, external claims, inference and unknowns are separate
finding types. Local source references must remain within the existing `sources/`
archive and match their checksum. Source acquisition and shared catalog curation
remain explicit research work; this adapter does not download or automatically
classify arbitrary source material.

Generated knowledge-map changes go through relay-knowledge commands. Publication
targets a PR in `stevetdp/superpod`, preserves its existing visibility, and does not
commit directly to its default branch. Index readiness requires the exact target
commit; submission or a queued indexing task is not reported as fresh knowledge.
Code and knowledge publication keep separate results because cross-repository
writes are not atomic.

Runtime files, `.lab/`, `.qualitygate/`, local configuration, databases, logs and
credentials are excluded from Git. Review any explicit export before publishing:
content checks catch some obvious leaks but cannot decide whether arbitrary prose
or evidence is suitable for disclosure. The current project supplies execution,
evaluation and delivery mechanisms; it does not establish that any prompt has
improved or that a new capability has emerged.

## Verification

```sh
bazel test --lockfile_mode=error //...
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
```

Tests exercise real temporary Git worktrees, package installation/rollback,
process cancellation, state recovery, prompt gates and Bubblewrap isolation.
Live workflow protocol tests are separately opt-in via `LAB_WORKFLOW_BIN`; their
ignored status is not evidence that a remote service or model has passed. Formal
delivery additionally requires a complete full Qualitygate check of the final
snapshot. GitHub Actions runs the Bazel build/tests and Rust formatting/lint checks.

Build tasks (write tasks or tasks explicitly requiring `qualitygate-cli`) seed
private dependency caches before claiming an attempt, in an idle host budget
window. Cargo imports only locked crates.io archives and their public sparse
index entries. Bazel imports hashes reachable from the frozen lockfiles and
checksum-verified registry and Rust/Python rule metadata. Each archive is verified again;
no credentials, host lockfiles, extracted repositories or writable hardlinks are
shared. Missing cache entries remain explicit in `build-cache-seed.json` and may
still require working network access. The marker records preparation, not a
passing build or gate. Limits apply to both copied and reused files (4096 files,
2 GiB total, 512 MiB per file); deadline exhaustion resumes without a claim.

Task build caches accumulate independently. Once an attempt is terminal and no
worker or reconciliation uses it, its `runs/<run-id>/cache` and
`runs/<run-id>/home/.cargo` directories may be removed. Retain the seed report,
process logs, receipts, target evidence and worktree until their retention policy
allows removal. The controller does not automatically delete these artifacts.

The real cache integration probe keeps the production namespace boundary and
requires offline Clippy plus Bazel build/package with repository downloads
disabled. Full controller tests run in the trusted host gate: their isolation
regressions create new namespaces, which the agent's outer `--disable-userns`
boundary intentionally denies. An inner test failure is not a passing host gate.

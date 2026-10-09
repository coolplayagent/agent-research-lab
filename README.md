# Agent Research Lab

A Rust + Bazel controller for AI-SDLC and long-horizon agent research. It runs
different Codex models in isolated task worktrees, preserves experimental
bindings, evaluates prompt variants, and provides durable delivery adapters for
CLI installations, GitHub PRs and research publication.

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
tree/runtime SHA-256 digests, and latest release tag/ID (or the latest source commit
when no stable release exists). The Rust types `freshness::SkillsManifest` and
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

Each idle admission batch verifies freshness before starting up to three agents of
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

Computer Use and repo-sandbox need their own configured, working targets. A
successful `--help` or version probe does not prove a browser, desktop or remote
sandbox is ready. Missing declared tools block the affected task. This controller
does not create personal desktop sessions or provision remote infrastructure.

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
is the default. `required_tools` declares admission prerequisites; `use_memory`
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

The default ceiling is three concurrent agents and 43,200 active wall-clock seconds
per Asia/Singapore calendar day. Concurrent agents share the wall-clock allowance;
it is not a dollar or token cap. Budget state survives controller restarts. Run the
bounded cycle and inspect its results before starting a service:

```sh
bazel-bin/agent-research-lab --config local.toml run --continuous --seed
```

`examples/agent-research-lab.service` is an optional systemd user-service template.
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

These public Rust types in `src/delivery.rs` and `src/knowledge.rs` are the JSON
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
cargo clippy --locked --all-targets -- -D warnings
```

Tests exercise real temporary Git worktrees, package installation/rollback,
process cancellation, state recovery, prompt gates and Bubblewrap isolation.
Live workflow protocol tests are separately opt-in via `LAB_WORKFLOW_BIN`; their
ignored status is not evidence that a remote service or model has passed. Formal
delivery additionally requires a complete full Qualitygate check of the final
snapshot. GitHub Actions runs the Bazel build/tests and Rust formatting/lint checks.

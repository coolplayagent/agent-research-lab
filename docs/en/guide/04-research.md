# 04 Research configuration and execution

[Documentation](../00-index.md) · [Section index](00-index.md) · [简体中文](../../zh-CN/guide/04-research.md)

## Configuration and tools

The Linux research host needs Git, Bazel, Rust, Bubblewrap with user namespaces, configured execution tools and GitHub CLI. Complete [quick start](01-quick-start.md), then copy [examples/lab.toml](../../../examples/lab.toml) to an untracked `local.toml`. Set the workspace, private state directory, existing SuperPOD checkout, Codex executable, published workflow CLI and all seven tool bindings. Example machine paths must be replaced with real Git checkouts. Configure model/API credentials through installed tools, never tracked examples.

Use an optimized build for sustained execution; unoptimized full skill-tree hashing can consume the host time budget:

```sh
bazel build -c opt --lockfile_mode=error //:agent-research-lab //:package
```

## Freshness and tool evidence

Production defaults to `require_latest = true`. `skills_manifest` points to an absolute, host-owned JSON file recording all seven installed skill directories, runtime paths, tree/runtime SHA-256 values and latest release tag/ID or default-branch source commit.

When upstream has a stable release, a newer local source build additionally requires `source_qualitygate: {path, sha256}` binding that source commit to a complete, nonempty, passing full diff report. Retain provenance and identify development builds; without that evidence, normal release requirements still apply.

`freshness::SkillsManifest`, `SkillSnapshot` and `freshness::skill_tree_digest` define the structure and digest algorithm. Populate evidence only after installation and verification. Five tools use upstream skills; project-owned `skills/` adapters cover Computer Use and Relay Memory. Runtime binaries are locally packaged and not tracked here.

```sh
bazel-bin/agent-research-lab skill-digest /absolute/path/to/installed-skill
bazel-bin/agent-research-lab --config local.toml refresh
```

Tree digests do not replace release/download verification. The controller checks current GitHub stable releases and installed bytes, fetches default branches and creates separate read-only source worktrees without switching developer branches. It records latest release and latest source independently. Reproduce old-binary failures on the pinned current-source build before calling them current defects.

Unavailable networks, changed skills or advanced upstream commits block admission without stale-checkout fallback. After a release changes, install/verify it and update the host manifest. Independent skill/repository checks run in batches of at most four under one absolute deadline; started checks settle before errors return, and partial results never replace the input manifest.

Before admitting up to eight agents in one experiment batch, the controller checks freshness and freezes source. Independent heartbeats renew leases while it prepares other agents or processes results. Launch and settlement rehash skill/runtime bytes; concurrent installation changes invalidate results. Source, skill, configuration or research-input changes produce new daily task IDs; historical experiments remain evidence. Idle continuous mode checks new batches every 15 minutes. `require_latest = false` is only for offline synthetic fixtures, not current-version research evidence.

| Binding | Purpose |
| --- | --- |
| `workflow-cli` | Durable jobs, claims, leases and evidence settlement |
| `relay-knowledge` | SuperPOD map validation, routing and exact-commit indexing |
| `into-markdown` | Research-source conversion |
| `qualitygate-cli` | Complete snapshot-bound delivery verification |
| `computer-use-cli` | Isolated desktop/browser experiments |
| `relay-memory` | Explicit local, isolated task continuity |
| `repo-sandbox` | Tasks in configured execution environments |

## Start research from a group

Enable research in the same service and private state, after stopping the previous core process:

```sh
bazel-bin/agent-research-lab --config local.toml serve --listen 127.0.0.1:8090 --max-seconds 43200
bazel-bin/agent-research-lab --config local.toml serve-link
```

OutcomeHub remains the home page. The adapter discovers configured people by stable ID and preserves their IM names. Create a group and choose CLI research, a configured repository and members in the goal form. Admission failure marks the scenario unavailable and retains bounded private diagnostics in `private/im-admission/`; the UI shows stable status/result codes. Each assignment becomes an immutable, read-only Job; source/prompt/policy/SuperPOD evidence returns to the group. Host acceptance and existing implementation/publication gates remain required.

`/apps/console/` inspects these tasks, historical runs and research-person settings. IM owns conversations, members and goals.

## People, execution preferences and sessions

Select a person, graph node or author to open a separate session window without losing the group. Public messages/results remain visible; consecutive tool actions fold into process cards showing commands, output and normalized stdio JSON. Expand long messages on demand; errors retain summaries. Page backward/forward through history; reading old pages pauses live following, and returning to latest resumes it. Graph, shared chat and session windows can expand; open details survive refresh/resize.

Mentions and person panels expose model, backend, tasks, memory, required tools, activity and evidence. Unique display aliases stay consistent across views. Identity lives in private `people/directory.json` with nonblocking locking and atomic writes; importing old `dashboard/personas.json` preserves people, tasks, sessions and memory.

Ordinary research reuses fixed researchers, critics and synthesizers across topic groups. Model/task changes do not change person IDs. Groups deduplicate people; person windows include all associated tasks/attempts, even beyond the latest 512 observed jobs. Alerts/recovery records use a separate bounded system area.

Profiles manage Soul and execution preferences. Create fixed, temporary or research people; converting temporary/research people to fixed preserves ID/history/memory. Edit unique names, Soul and purpose, and choose default people per task role. Expertise describes perspective, not model or permissions. One person may fill multiple roles, but independent implementation review requires another person. Optimistic configuration revisions affect only new batches.

The execution page selects a configured backend/model or inherits task defaults. The host freezes preferences in new Jobs and records actual model, backend spec and executable digest. Edits do not rebind existing tasks, retries, memory or sessions. Unknown models/backends or insufficient capabilities fail; declared capability is not provider compatibility or quality evidence. `[models]` retains role defaults; optional `[backend_models]` lists model IDs per backend. The UI exposes IDs/models/capabilities, while paths, arguments and credential environment names remain host-side.

Sessions use `task.id`, with distinct `run_id` attempts. History and bridge requests retain the frozen person identity. Authorized task/session endpoints decouple the crystal from coding-agent implementations; changing technology does not change message permissions or private audiences. Native activity appears as folded process details. Manual task registration may specify `"persona_id":"person-…"`; omission preserves existing task/experiment isolation. Local management accepts bounded same-origin JSON with an explicit intent header, not arbitrary commands.

The inspector can collapse groups, directed chats and people; pinned groups come first. Hide or resize the sidebar by dragging, keyboard arrows or double-click reset. Shared-chat collapse retains conversation/scroll state. Browser preferences persist widths and section proportions; narrow screens stack the sidebar and retain resizable person overlays.

## Memory, prompts and evidence

Each person has `people/memory/<person-id>`. The memory page calls configured relay-memory for recall, statistics and explicit notes; it neither builds another memory engine nor edits SQLite directly. Registration freezes bounded `prepare` context, data packet, executable digest and identity revision into the Job/prompt digest. Agents do not mount shared person memory; retries reuse captured context. Independent formal experiments avoid this optional identity-memory path, and task-local memories remain isolated.

After authoritative workflow success, the host writes a brief `remember` checkpoint with source, prompt, policy/config, SuperPOD and receipt references. Public results/notes may become future context; private chats, raw runs, credentials and holdouts are excluded. Completion journals prevent repeated successful writes. Ambiguous CLI exit keeps the operation unknown with a system notice; reconcile against relay-memory before retrying. Personal memory is historical evidence, not new authority or proof of a finding.

Soul and role guidance display separately. Role guidance comes from frozen lab source or a registered prompt, using curious research, careful criticism, practical implementation and fair evaluation. Public reasons/questions may express uncertainty; the UI preserves original text without inventing emotion/evidence for history. On-demand identity endpoints read known tasks only; role text is capped at 16 KiB and Git reads at two seconds. Missing historical sources are explicit.

## Explicit bounded research cycles

```sh
bazel-bin/agent-research-lab --config local.toml enqueue examples/task.json
bazel-bin/agent-research-lab --config local.toml run --max-seconds 900
bazel-bin/agent-research-lab --config local.toml status
bazel-bin/agent-research-lab --config local.toml report
```

`enqueue` binds IDs to inputs, source, SuperPOD, prompt and configuration; an ID cannot bind different inputs. Tasks use isolated worktrees and dependencies receive validated completion reports. Default `write: false`; `required_tools` controls admission, including follow-ups. `use_memory` enables task-local checkpoints and immutable receipt references. Private receipts retain full source/skill snapshots; `prompt_version` selects a registered prompt explicitly.

`seed` or `run --seed` adds daily independent research, criticism and synthesis for delivery, recovery/memory, collaboration and desktop/sandbox tasks. Synthesis may propose bounded follow-ups in allowed repositories; implementations may request read-only review. The controller caps proposal count/depth. Model proposals never authorize installation, remote writes, publication or merge.

```sh
bazel-bin/agent-research-lab --config local.toml pause
bazel-bin/agent-research-lab --config local.toml resume
bazel-bin/agent-research-lab --config local.toml run --max-seconds 900
```

Pause stops admission/cancels subprocesses; resume clears the marker. Restart reconciles durable attempts with real process identity. Read-only failures have bounded retries; `retry TASK_ID` explicitly retries eligible tasks. Interrupted writes stay uncertain until reconciled. Recovery does not resume model instructions at an exact instruction boundary.

Retained launch records reuse original timeouts/input envelopes even after controller defaults change. Successful workflow receipts can reconstruct post-crash completion work; follow-up admission replays, but unreconciled memory writes remain unknown. Completion, freshness and seeding share the daily budget and run deadline.

At most eight agents share 43,200 seconds of active wall time per Asia/Singapore calendar day. This is neither money nor a token cap, and accounting survives restart. Inspect a bounded cycle before explicitly starting continuous seeding:

```sh
bazel-bin/agent-research-lab --config local.toml run --continuous --seed
```

Do not run continuous seeding alongside the unified service on the same state. See [operations](07-operations.md) for service templates and diagnostics.

## Explicit recurring CLI research

The existing research scheduler can run a host-configured campaign in one existing
conversation. The optional private `state_dir/private/research-campaign.json` file
defines `schema_version: 1`, an `id`, `group_id`, four distinct `person_ids`, four
configured CLI `repositories`, `objective`, `acceptance`, four member
`instructions`, `max_seconds`, `pause_seconds`, `failure_backoff_seconds`, and
`max_rounds`. The operator must create the group and research identities first.
At least four worker slots and the ordinary research admission prerequisites are
required. No campaign runs when the file is absent.

Each round creates an immutable group goal with four parallel assignments and
rotates to the next configured CLI. Existing goal records recover the cursor after
a restart and make repeated admission idempotent. The complete campaign policy
digest is frozen in the goal and research prompt. Changing that policy under an
existing campaign ID is rejected; a changed policy needs a new ID. Failed or expired
assignments delay the next round. The ordinary daily budget, source and SuperPOD
pins, freshness checks, isolation, and storage capacity remain authoritative.

Group task cards show the four actual member states and let the operator expand
each member's research process, refresh its public events, and page backward.
Process requests resolve the group's current goal assignment and attempt through
the storage API before reading its bounded execution projection. They do not expose
raw logs, reasoning, prompts, or credentials. Member contributions and result
evidence remain in the group transcript.

Deleting the private campaign file stops future round admission; existing bounded
assignments finish under their original limits. Cancelling a campaign goal stops
later rounds and follows the existing active-assignment cancellation rules. A
submitted contribution permits further exploration, but does not accept the goal,
promote a version, authorize a write, or approve a merge. Code delivery is a separate
host process: reproduce findings, independently review candidates, run repository
checks and full Qualitygate, then obey exact-head remote review and CI gates.

## Cohort preparation and compact status

In an idle window without active or retained launch records, the scheduler prepares the next selected cohort before claiming execution leases one by one. Concurrency, dependencies, research versions and exploration tracks bound the cohort; failed tool preparation consumes no execution slot. Serial desktop/cache preparation may delay the first launch and counts toward run/daily budgets. Launch rechecks versions, pause and remaining time.

```sh
bazel-bin/agent-research-lab --config local.toml status --summary
```

For repeated monitoring, `status --summary` returns only `paused`, `budget`, controller-classified `summary`, `total_jobs` and `details_included: false`, without per-task details, prompts or workflow receipts. Observation remains sequential rather than atomic; use `status` or `report` for details.

## Bounded batch registration

`agent-research-lab --config local.toml enqueue-batch request.json --max-seconds 300`
registers an ordered batch of 1..32 ordinary native Tasks. The request is
`{"schema_version":1,"batch_id":"owned-batch","tasks":[...]}` and must be a
regular JSON file at most 1 MiB. Dependencies must already exist or precede the
Task in the batch. Latest-baseline mode is required.

Each invocation collects fresh inputs once. A private immutable plan under
`state_dir/private/enqueue-batches/<batch_id>/plan.json` freezes Task selections,
persona memory, configuration and complete source/skill/knowledge inputs before
registration effects. Replay uses the same canonical request and frozen choices;
it still performs one fresh collection and rejects real binding drift. Only
repository/skill acquisition timestamps are excluded from identity comparison.
The actual serialized plan is limited to 16 MiB.

Registration holds the batch lock and one native enqueue lock. All existing
bindings and interrupted worktrees are checked before the first new registration.
A failure retains partial native state and returns a nonzero structured response
with `code` and `registration`; the private receipt distinguishes a recorded Job
from confirmed workflow registration. If receipt persistence fails, the returned
observation has `receipt_persisted:false`. Retry the same request after diagnosing
the failure; no automatic retry, rollback or Job overwrite is performed.
New workflow bundles are submitted during registration, so an owner rejection
can leave a registered prefix even after native Task and binding preflight passes.

Existing current attempts are confirmed through owner `verify`, `history-at` and
pure `kernel check`. Confirmation never repairs an advanced/live attempt, resets
it to attempt 1, acquires a lease or starts a worker. A missing or unsupported
owner proof remains incomplete. Complete registration does not mean Task success;
a confirmed owner run may still be running, failed or cancelled. Concurrent
controllers may observe the registered prefix; this is not atomic cohort admission.

The 1..600 second allowance is shared across acquisition, preparation and native
calls. Checkpoints stop later effects, while synchronous hashing/filesystem sync
cannot be preempted by the in-process deadline. Ordinary `enqueue`, collaboration,
seed and run admission retain their existing behavior.

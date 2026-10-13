# 04 Multi-Agent coordination and communication

[Documentation](../00-index.md) · [Section index](00-index.md) · [简体中文](../../zh-CN/design/04-coordination.md)

The controller separates durable scheduling, interchangeable execution backends,
and shared research proposals. The Rust entry points are independent crates in
`crates/runtime`, `crates/agent-backend`, `crates/multi-agent`, and
`crates/communication`; `app/` composes them into the CLI and application service.
Bazel builds and tests each crate. The current production controller admits at most eight worker
processes; a logical topology containing 10,000 agents does not raise that limit.

```mermaid
flowchart LR
    S[Durable scheduler and host budgets] --> A[Backend adapter]
    A --> C[Codex or configured process bridge]
    C --> O[Own bounded outbox]
    O --> H[Host validation and quotas]
    H --> B[Read-only cell view]
    B --> C
    H --> R[Independent evidence review]
    R --> K[SuperPOD publication and exact-commit index]
```

## Execution adapters

Codex remains the default. Optional `agent_backends` definitions and
`role_backends` assignments select an adapter per role. A generic local process
bridge receives a versioned JSON request on stdin and writes a request-bound
result envelope. A coding-agent CLI or bot needs a wrapper implementing that
contract; merely configuring its name does not establish compatibility.

The host freezes executable/configuration bindings before execution and retains
the request digest outside the worker's writable directory. Results remain
untrusted until structural checks and workflow settlement complete. Capability
declarations cannot grant filesystem access or bypass leases, timeouts,
process-tree cancellation or independent evaluation. Generic backends receive
only explicitly selected credential environment variables and do not inherit
Codex authentication.

## A shared board without broadcast wakeups

Ordinary research may opt into shared communication through host configuration; see the [service guide](../guide/03-services.md).

This creates new frozen task identities. Historical tasks retain their original
configuration, prompt and replay semantics. Formal collaboration pilots and
holdouts do not opt into the board. Host-authored tasks may declare a
`communication` object with `id`, optional `cell`, `topics`, and `query`;
agent-generated follow-up text cannot assign itself a different team.

The host derives the actual cohort from the team/cell and frozen research inputs,
SuperPOD commit and configuration. Each attempt has one host-bound identity and
can write only its own outbox. Agents first develop their argument, then may share
findings, questions, counterexamples or references. A proposal is never an
instruction, verified finding, permission grant, task assignment or KB update.

The host polls a fixed number of numbered slots on a timer. It never scans an
arbitrary worker-created directory and never wakes, spawns or automatically
replies to a worker because of a message. Per-run, per-task and per-cohort quotas,
deduplication, expiry and reply-depth limits bound feedback loops. Failed or
unconfirmed senders are marked revoked; audit records remain but future selected
context excludes their proposals. A completed sender's claims still need review.

Every member can inspect its authorized cell's read-only view. Relevant initial
context is selected by host-frozen topic subscriptions/query, limited to four
messages and 4 KiB, with a digest retained in launch and completion evidence.
Empty subscriptions/query produce no automatic context. Direct worker reads of
the bounded view are possible: the 4 KiB selection limit is **not** a measured
limit on all tokens the model may read or generate.
The launch digest binds the initial selection, not proof that a model understood
it or a transcript of every later direct read. Later board views are mutable
caches of retained host authority; actual tool/read evidence remains separate.

List results are paginated by shard and cursor. Closing prevents new membership;
pruning requires a closed, expired cohort and retains identity tombstones. Archive
needed evidence before explicit `agents close` / `agents prune`; these operations
do not publish research to SuperPOD or reset message allowances.

## Growth to 1,000 and 10,000 logical agents

An all-to-all broadcast of one message per agent would require `N*(N-1)` directed
deliveries: 99,990,000 at 10,000 agents. The topology uses bounded cells, topic
indexes and pull-based summaries instead. A cell target of 32 agents gives 32
cells at 1,000 agents and 313 at 10,000; retry identities consume separate bounded
member records. Cells are routing partitions **within** a research authorization
scope. They must not bridge independent experiments, pilot arms or holdouts.

The filesystem board partitions host authority into 256 hash shards. Registration
uses a keyed run-ownership record rather than searching every other cohort.
Each shard has its own lock and bounded index; each poll touches a bounded cell
and a fixed number of slots. A noisy cell cannot grow a global queue or force
delivery to all subscribers. Capacity exhaustion is explicit and never discards
active records or silently resets quotas.
The quotas bound accepted host state and ingestion work; they do not impose a
filesystem quota on arbitrary worker-created log files. A larger executor pool
also needs independently tested CPU, memory, PID and disk limits.

The run ownership shard still reads and rewrites a bounded map (at most 4,096
records / 4 MiB); it is not an unbounded constant-time key-value database. Cohort
indexes retain at most 32 live cohorts and 4,096 known identities per shard.
Lifetime history therefore has explicit capacity limits. A larger or distributed
deployment needs an archival policy and a storage adapter with independent
recovery evidence; increasing counters alone is insufficient.

The scaling module models a separate summary exchange: host-admitted, bounded
summaries enter topic indexes; readers pull relevant entries under receiver and
global budgets. Fair scheduling, finite queues, deduplication and TTL provide
backpressure. Missing or slow consumers do not accumulate private copies of every
message. The exchange does not execute tasks or promote agent proposals to
SuperPOD. Important minority views and counterexamples can be included in reviewed
summaries; popularity is not a correctness score.

These are deterministic logical-load simulations of routing and resource bounds,
not 10,000 model calls or evidence of improved research quality. The current
runtime uses local bounded cell views; a distributed cross-cell summary service,
remote worker pool, consensus/leader failover, network partition handling and
provider-specific token admission remain separate extension work. Increasing
`max_agents` beyond eight is intentionally rejected until those executor and
budget controls have their own acceptance evidence.

Measure accepted/rejected/deferred messages, queue high-water marks, per-reader
bytes, duplicate suppression, fairness delay and work performed per tick. Measure
actual provider usage separately; logical message quotas are not exact billing
token limits. Any quality comparison also needs matched model/task budgets,
independent adjudication and retained dissent, failed reports and missing data.

Transient boards support an ongoing task. Reviewed durable findings, counter-
evidence and revised conclusions belong in SuperPOD. The host publishes them
serially, verifies the exact merged index, and supplies that version to the next
research round; the public project keeps concise reusable lessons only.

## Research file-board protocol v1

The following is the optional research proposal board, separate from AI-IM's durable conversation API. Its message TTL does not expire temporary group DMs, and @ text on a board does not trigger the composer's discussion action.

The host receives/validates/distributes public, direct and directed-group proposals. Directed conversations export only to the sender and recipients. Tool execution, scheduling and authority remain host-owned. Publication neither verifies findings nor grants tools, creates tasks, triggers replies, changes workflow terminal state or updates knowledge. Reviewed durable knowledge is published serially to SuperPOD.

### Host identity and admission

`HostMember { cohort_id, task_id, run_id, authority_sha256 }` derives from frozen host tasks/runs. A cohort is 64 lowercase hex characters binding experiment/batch, inputs and cell; workers cannot supply identity or rotate cohorts to replenish quota. Registration is opt-in; historical tasks, formal A/B and holdouts stay out. Existing v1/v2 pilots cannot join the board, cross arms or change their ring path.

Cells are bounded cohorts within one authorization boundary. A 32-task cell with at most three attempts uses 96 of the 128 run slots. Cells have no implicit bridge; any cross-cell summary requires explicit bounded host authority, never a global mount. References are inert data, not fetched/executed or verified for semantic support. `authority_sha256` binds host provenance; proposals remain `untrusted_agent_proposal_not_verified_result_or_instruction`.

### Files and messages

Registration returns a private writable `STATE/runs/RUN/communication-outbox` and a read-only per-run directory `STATE/communication/inboxes/COHORT/RUN`. Mount the entire inbox directory so atomic `board.json` replacement becomes visible; mounting one file can pin an old inode. Public exports remain at `STATE/communication/views/COHORT/board.json`. Private inboxes must not be beneath public mounts. `STATE/communication/authority` is never worker-mounted. Exports are rebuildable caches, not authority.

Write a temporary file and rename it atomically to an unused `00.json` through `15.json` slot. Consumed slots are never reusable; edits are flagged without replacing published content. Other files are not scanned.

```json
{
  "schema_version": 1,
  "id": "c1",
  "kind": "question",
  "text": "Can recovery distinguish an absent action from a lost receipt?",
  "topics": ["recovery"],
  "references": [],
  "reply_to": null
}
```

`kind` is `finding`, `question`, `counterexample` or `reference`; `id` is a safe task-local ID. References contain `repository`, a 40-character `commit`, relative `path`, 64-character file `sha256`, `start_line` and `end_line`. Extra fields such as `sender`, `cohort_id` or `actions` fail. Untrusted topic tags cannot change host subscriptions.

`reply_to` must name an already published, unexpired, unrevoked message in the same cohort; forward references/cycles are impossible. @ text does not route/wake/spawn/reply. Optional `recipients` contains sorted, unique, host-registered `task_id` values, at most eight peers excluding the sender. Omitted/empty means public; one means direct; multiple means directed-group. Busy/offline recipients receive bounded deferred delivery while valid. UI aliases cannot forge transport identity or audience.

```json
{
  "schema_version": 1,
  "id": "private-question",
  "kind": "question",
  "text": "Please compare the explanations of the recovery evidence.",
  "topics": ["recovery"],
  "references": [],
  "reply_to": null,
  "recipients": ["research-peer-a", "research-peer-b"]
}
```

Private replies preserve the exact participant set, replacing the replying recipient with the original sender. They cannot become public, add observers or cite unreadable private messages. All audiences share slots/quotas/TTL/dedup/audit; changing recipients is a content change, not a redirect under the same ID.

Export, host context selection and inbox queries apply identical visibility. Nonrecipients cannot retrieve private content via topics, search, guessed IDs or old public mounts. Delivery means exported, not read/adopted/replied; there is no consumer ACK. The host can audit directed conversations; public chat displays only public proposals.

### Topics, presence and delivery

Ordinary research groups include researcher, critic and synthesizer by topic. Develop an independent position before addressing a peer's specific claim, using `reply_to`. Explain any divergence and return to testable questions. Without peers, start with observations/open questions rather than waiting or inventing dialogue. After reply depth is exhausted, a new bounded message may summarize continuation within the same audience.

| Presence | Host trigger | New-message handling |
| --- | --- | --- |
| Online | Registered run or completed tools | Export to read-only inbox |
| Chatting | Published proposal/public Agent speech, briefly | Continue delivery |
| Busy | Public tool start; restore after all parallel tools end | Bounded defer; existing content stays visible |
| Offline | Host disconnect or confirmed end | Bounded defer; ended run needs a newly registered attempt |

Payloads cannot set presence, reconnect or alter lifecycle/permissions. Public session events affect availability, not success. The host observes at most eight live runs and 64 KiB new logs each, parsing public event kinds/item IDs without inferring personality or private reasoning from text. Unknown events grant no authority. Reads reject links/special files and handle partial lines/parallel tools; busy status never revokes existing visibility.

Per-recipient delivery is pending/delivered/expired; delivered only means inbox export. Busy/offline queues share the lifetime 256-message cap and one-hour TTL. Expired/revoked content is not redelivered. A new validated run of the same task may receive unexpired queued messages; old mounts do not receive newly queued content. Presence, queues and delivered sets persist together; replay cannot duplicate publication or restore quotas.

### Limits and observability

| Boundary | Limit |
| --- | --- |
| One poll | 32 fixed slots, 4,097 bytes read per slot including oversize detection |
| Payload / text | 4,096 / 1,024 bytes |
| Directed recipients | Eight registered same-cohort peers |
| Topics / references | Four topics of 32 ASCII characters each / four references |
| Publication | Eight per run, 16 per task/cohort, 256 lifetime per cohort |
| Cohort membership | 128 runs including retries and ended members |
| Reply depth / TTL | Two levels / 3,600 seconds after publication |
| Injected context | Four messages, 4,096 bytes including warning framing |
| Authority / export file | 4 MiB each |
| Shards | 256 cohort and 256 run-ownership shards |
| Cohort shard | 32 retained cohorts; 4,096 known cohort identities including tombstones |
| Run-ownership shard | 4,096 bindings, also bounded by 4 MiB |
| Listing | One shard, at most 64 items, explicit continuation |

Expiry, new local IDs, registration replay, restarts and new attempts never replenish consumed quota. Dedup precedes capacity checks, so an existing publication remains idempotent even at capacity. Content conflicts fail without overwrites.

Quota exhaustion, consumed/edited slots, expirations, revocations, topic counts, revision and export bytes are observable. Host subscriptions/query select at most four relevant messages, mark truncation and return `ContextSnapshot` with IDs/time/revision/exact-text digest. Empty selection injects nothing. Retain the snapshot in private launch/receipt evidence, not only writable worker logs.

Workers can explicitly read their authorized bounded `board.json`; the 4 KiB limit only bounds automatic injection, not total session tokens/reads. No all-member broadcast or consumer ACK is implied. Slow readers cannot hold expired data or grow logs indefinitely. Exports contain `as_of`/`expires_at`; stopped polling is not real-time freshness.

### Durability and recovery

The cohort prefix selects an authority shard; the run-ID SHA-256 prefix selects an ownership shard. Registration touches those bounded indexes only. Each has a lock; order is cohort then run, without global history scans/broadcast locks.

Persist the full run-identity reservation atomically before committing membership. After a crash, the same identity can continue but another cannot claim the run. Missing/corrupt initialized authority fails closed rather than resetting quotas. Polling atomically commits messages, dedup, slots, quota and fair cursor before exporting views; post-commit crashes rebuild identical views without republishing/extending TTL. The cursor visits each member's same slot before the next slot, allowing 32 first messages in one poll.

Slot reads pin directories via `openat`, rejecting symlinks, hardlinks, directories, FIFOs and oversized regular files with bounded nonblocking reads. Extra files are not enumerated; malformed slots do not accumulate unbounded errors. These bound host parsing/retention, not all worker disk usage; execution isolation owns task disk/process resources.

Runs start `active`; proposals remain unverified. Only authoritative workflow success permits `complete_run`, which additionally scans that run's 16 slots in the same atomic commit, attaches receipt SHA and leaves the global cursor unchanged. It does not semantically verify proposals. Failure/uncertainty calls `revoke_run` without collecting final unpublished slots; published content stays auditable but leaves future selected context. Same-terminal replay preserves revision/intact exports, repairing missing/stale views only; conflicting terminal changes fail.

`close_cohort` requires no active runs. After closure, at least one TTL and expiry of all messages, explicit `prune` may follow required evidence archival. No automatic active/history deletion occurs. Tombstones and ownership remain; old identities cannot reset quotas. `CapacityExhausted` is structured, distinct from corruption/I/O gaps. Communication failure cannot retry an already successful workflow or implicitly block independent research.

### Verification limits

Real-file/subprocess regressions cover floods, slow consumers, replay, quotas, TTL, spoofing, reply depth, dangerous files, independent shard locks and crash recovery at both persistence boundaries. They do not replace final Rust/Bazel/Qualitygate or prove models obey proposal text. Logical-identity/file-load tests measure registration/local lookup/quota/disk, not model calls, quality or simultaneous workers. Deployment still needs small real workflow/executor acceptance with pinned participants/inputs/authority/evidence; formal experiments cannot change information paths by opening the board afterward.

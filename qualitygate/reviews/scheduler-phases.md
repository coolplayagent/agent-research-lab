# Scheduler phase review

Baseline: `0e64320d62b30b82c3f017a6c0a146eb6f4a4802`.
This is a local implementation and semantic review, not independent signed
approval. Acceptance requires full snapshot-bound Qualitygate and repository
tests. No policy, source freshness, receipt, SuperPOD or attempt gate is relaxed.

## Ownership

The public runtime functions still dispatch through `scheduler`. Its private
implementation is now a directory with three supporting owners:

| Owner | State and authority |
| --- | --- |
| `scheduler/mod.rs` | Controller lock and observer, one run window and ledger, active/idle ordering, scoped host effects, proof preparation and the guarded-failure latch. |
| `scheduler/cohort.rs` | Desktop readiness and per-run build backoff. Preparation borrows the controller's ledger, window and accounting timestamp; it returns candidate jobs plus retry/defer information. It does not acquire leases or spawn workers. |
| `scheduler/workers.rs` | Active jobs, their guarded write snapshots, processes and lease heartbeats, plus communication poll timing. It cancels, observes and settles those retained attempts. It cannot enqueue or launch new tasks. |
| `scheduler/eligibility.rs` | Existing workflow/dependency admission checks and one shared cohort-order rule used before preparation and before launch. |

These are in-memory operation boundaries, not new serialized lifecycle states.
State enums remain in `contracts`, records remain in `task`, and the research
workflow client retains its original storage boundary. No dependency enters core
collaboration and no vendor-specific behavior enters the scheduler.

## Semantic review

- A candidate joins only the first candidate's exploration track. A new sandbox
  candidate cannot follow an earlier peer; ordinary peers can follow a sandbox
  candidate that was selected first. The single predicate preserves that existing
  asymmetry and is used at both preparation and live admission.
- Preparation still respects capacity, scope, retained launches, retry time and
  workflow/dependency state before external prerequisites. Preparation failures
  take no capacity. Source freshness is rechecked after all selected probes and
  before the host proof or first lease. Unknown workflow states remain blocking.
- The admission deadline still encloses preparation and launch. Prerequisites
  borrow the existing run window; they cannot restart it. Host budget charges,
  accounting timestamp updates and retry/backoff timestamps retain their order.
- Worker polling preserves process cancellation, heartbeat failure detection,
  current-job confirmation, exit observation, lease-fenced settlement, heartbeat
  stop, lease release and final guarded persistence. Active guard field order is
  unchanged, preserving process/heartbeat destruction behavior on early returns.
- Polling reports a guarded failure; the controller combines it with its existing
  failure latch using `|=`. An empty or successful later poll cannot clear a
  prior failure. Pause/budget cancellation and uncertain settlement still prevent
  automatic guarded retries. A changed current attempt is never overwritten.
- The main loop retains startup recovery before budget admission, scoped
  follow-up/outbox rules, idle refresh timing and the same bounded sleep. The
  identical active/idle sleep branches are replaced by one call after the branch.

## Evidence and limits

The extraction audit compares prerequisite and worker-poll bodies after explicit
borrow/field alias normalization, and compares unchanged public entrypoints and
workflow admission. The shared predicate's equivalence is separately reviewed
above; the audit alone is not behavioral evidence for orchestration.

Existing runtime tests preserve all 94 cases and their ignore settings. Five new
regressions exercise sandbox ordering through the real scoped scheduler and
active worker settlement through real bounded child processes. They check that
failed workers release once and never retry under guarded admission, current
attempt drift preserves the newer record without publishing an old completion,
and ambiguous settlement retains the original launch for reconciliation.
Only the external workflow and harmless worker programs are fixtures.

Full Bazel tests additionally cover API serialization, contracts/locales, core
startup and package boundaries. The 35 existing ignored runtime integration
tests remain ignored; deterministic fixtures do not certify live external
providers or deployment readiness. Snapshot-bound Qualitygate covers configured
checks and selected inputs, not every possible runtime environment.

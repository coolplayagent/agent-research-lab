# Delivery outbox boundary review

Baseline: `d003736f59776a84a7e52debf355319dfafc557d`.
This is local implementation and semantic review, not independent signed approval.
Delivery requires repository tests and full snapshot-bound Qualitygate.

## Ownership

| Module | Responsibility |
| --- | --- |
| `protocol` | Existing operation tags and queue record fields; re-export the shared `QueueState`. |
| `store` | Own the canonical root and exclusive lock; retain immutable request bytes, enumerate IDs and atomically replace state records. |
| `provenance` | Validate host-owned installation/merge evidence and reviewed knowledge report sources. |
| `dispatch` | Invoke the optional adapters and interpret their typed receipts before serializing retained observations. |
| `queue` | Coordinate admission, one-operation ticks, crash recovery, backoff and explicit retry. |

The crate facade preserves its public API. The local `Outbox` lifetime now holds
its lock across the entire operation; queue code cannot call a persistence helper
without a store handle. The store validates entry IDs and their canonical request
paths before each read or write. This is an optional host-local delivery journal,
not the production collaboration database. No service API, vendor dependency or
core-startup requirement is added.

## State and recovery review

- Request identity still hashes the same compact serialized operation bytes.
  Repeated enqueue verifies the immutable request and returns its retained state,
  including terminal records. It does not reset attempts, results or deadlines.
- Directory permission and symlink checks are unchanged. Requests retain
  create-new writes, sync, read-only Unix permissions and directory sync; state
  updates retain temporary-file writes, sync, rename and directory sync. Store
  validation adds an explicit boundary for cross-store or forged retained entries.
- Admission, dispatch and explicit retry retain the same evidence validation.
  Required Qualitygate, snapshot, evaluation and skill artifacts still reside
  under host-owned `evidence/` and pass their existing release evidence gate.
  Knowledge preparation still revalidates reviewed report/source evidence.
- A tick holds the lock during selection, provenance checks, running-intent
  persistence, adapter invocation and settlement. Running intent and the increased
  attempt count reach durable state before effects. The internal dispatcher is
  `FnOnce`; each tick returns after one recovery/dispatch decision.
- Observing a retained running entry still marks it for reconciliation without
  invoking the adapter. Adapter failure still requires explicit reconciliation;
  no failure or interrupted write is automatically replayed.
- Previously, a pending immutable-request read/hash/decoding failure returned an
  error from every tick and could starve later work. It now durably marks that
  trusted queue entry `needs_reconciliation`, retains the original request bytes
  and does not increment attempts or dispatch. The next tick can select later
  entries. Explicit retry still refuses invalid request bytes or provenance.
  Invalid state identity/path/serialization is still an error: an untrusted state
  record cannot authorize rewriting or quarantine of an arbitrary target.
- Index readiness is read from the typed `IndexRefreshStatus.ready` field before
  serialization. `false` still waits 30 seconds; a due waiting entry can execute
  another bounded refresh. Complete and quarantined entries are not auto-retried.
- Merge observations are parsed into shared `PullRequestState`. Only `Merged`
  settles as complete; open, closed or unknown states retain the same reconciliation
  diagnostic. A PR receipt is not a confirmed merge. Raw results are serialized
  intact, and previous result/deadline retention on other transitions is unchanged.

No new lifecycle enum, string state union or frontend contract is introduced.
The existing queue and operation serialized values remain intact, including
historical knowledge-root aliases. All unit test bodies reside in sibling files.

## Evidence and limits

The extraction audit compares twelve areas: both wire records, initialization,
provenance, persistence, immutable-request reading, enqueue storage, sorted queue
enumeration, unchanged orchestration, explicit retry and all adapter invocations.
Store wrappers are expanded; the typed settlement and invalid-request quarantine
branches are reviewed separately. All three original test bodies are preserved.

Eight new regressions cover exact wire bytes and aliases, durable running intent
and lock ownership during dispatch, one-operation scheduling, refresh backoff,
corrupt/unknown request preservation without dispatch, continued queue progress,
store confinement, merge-state interpretation and raw index observation retention.
All eleven focused outbox tests pass. Full Bazel checks cover CLI/runtime callers,
shared contracts, locale boundaries and independently buildable packages.

Local fixtures and injected internal dispatch callbacks do not establish actual
remote installation, publication, merge or production deployment success.

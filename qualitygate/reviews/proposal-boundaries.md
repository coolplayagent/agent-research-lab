# Proposal board boundary review

Baseline: `52175c2e6f02a331a2ee0d96041d109a03a3452c`.
This is a local implementation and semantic review, not independent signed
approval. Acceptance requires full snapshot-bound Qualitygate and repository tests.

## Ownership

The existing research proposal board is still one host-owned file store. Its
public Rust API is re-exported from the crate root; it adds no service, database,
protocol revision, state enum or external adapter. Core collaboration storage and
conversation semantics are unchanged. Proposals remain untrusted research data;
they cannot schedule work, promote results or update an external knowledge store.

| Module | Responsibility |
| --- | --- |
| `board` | Coordinate registration, polling, lifecycle changes and reads; hold the cohort-shard lock through each mutation and publication. |
| `store` | Own authority paths, indexes, replayable registration state, run reservations, bounded persistence and view-file cleanup/publication. |
| `authority` | Define retained cohort/member records and validate identities, quotas, digests, reply lineage, timestamps and delivery authority. |
| `protocol` | Preserve public records, field order, serde defaults/omissions and syntactic validation. |
| `admission` | Consume fixed outbox slots once; enforce duplicate/conflict, run/task/cohort quota and reply checks before appending a message. |
| `audience` | Share visibility and fixed reply-audience predicates across admission, authority validation, presence and projection. |
| `projection` | Render bounded public/private views and frozen context without reading worker files or changing authority. |
| `files` | Preserve anchored directory traversal, no-follow/nonblocking reads and size/link checks. |
| `presence`, `limits` | Retain the shared host lifecycle and existing protocol resource bounds. |

`Board` composes one private `Store`; it does not create a separate mutable handle
for each module. The store does not acquire an additional cohort lock. Run-owner
reservation still takes its shard lock only while the caller holds the cohort
lock, preserving the existing lock order.

## Semantic review

- Registration reserves run ownership before committing a new member. Initial
  cohort index publication, authority persistence and the initialized marker keep
  their original order. Missing initialized authority remains corruption; a
  pruned identity cannot be reused to reset quotas.
- Polling advances the same fair fixed-slot cursor and commits authority before
  invoking the crash hook or publishing views. Slot digests, consumed outcomes,
  duplicate checks and quota counters are unchanged. Expiration does not reset
  lifetime quotas or permit replay.
- Every retained read still validates the complete cohort, including shared
  origin/presence states, message hashes, member binding, audience, reply lineage,
  quotas and delivered-message indices. Unknown states fail deserialization;
  they never imply active membership or successful completion.
- Successful terminal replay still repairs only stale exports. The final outbox
  drain occurs before the same terminal state commit. A terminal origin cannot
  change or reconnect without a new host-authorized run.
- Pruning commits the index tombstone before deleting inboxes, the public export
  or authority file. Replaying cleanup cannot revive the retired identity.
- View and context filters retain expiration, revocation, audience and delivery
  requirements. A context read neither acknowledges delivery nor grants membership.
  Reference paths remain syntactic; neither validation nor projection fetches them.
- Filesystem reads retain their exact bounds and flags. Opening existing authority
  does not create directories or change permissions. Proposal TTLs and private
  authority remain scoped to this optional research component.

## Evidence and limits

The extraction audit makes 87 comparisons covering record/enum definitions,
validators, public signatures and operation bodies. Store/projection delegations
are expanded before comparing the original operation order; private visibility,
borrow aliases and test-only store access are normalized. Presence behavior is
unchanged. These checks support the structural move and do not replace behavioral
or architecture review.

All 16 existing tests retain their assertions and ignore settings. They cover
real writer contention, unsafe files, quota/idempotency behavior, crash recovery,
shard locks, terminal replay, audience routing and presence queues. New tests pin
legacy public proposal bytes plus proposal/message digests, and exercise eleven
corrupt authority variants through public view, inbox and context reads while
checking that neither authority bytes nor exported views are rewritten.

Full Bazel verification additionally covers public API consumers, shared wire
contracts, frontend locale boundaries, core startup and packages. One existing
large-scale proposal test remains ignored. Passing fixtures and snapshot-bound
checks do not establish live deployment behavior or every filesystem environment.

# Delivery adapter boundary review

Baseline: `a04bbb404bbe5e779753b4e568eabf5173e5c580`.
This is a local implementation and semantic review, not independent signed
approval. Delivery requires repository tests and full snapshot-bound Qualitygate.

## Ownership

The crate root preserves existing public function and record paths. Private
modules add no service, database handle, external store or execution adapter.

| Module | Responsibility |
| --- | --- |
| `evidence` | Validate bound host artifacts and release source, prompt, policy, knowledge, evaluator and skill identities. |
| `package` | Hash regular file content and modes, reject package symlinks, copy staged packages and identify the skill. |
| `installation` | Hold one install lock across recovery, staging, activation, smoke verification and receipt settlement. |
| `github` | Fetch and decode remote PR/check/review observations; retain completeness and head bindings. |
| `pull_requests` | Reuse or create one PR for the exact pushed head. |
| `merge_gate` | Evaluate complete remote observations against frozen release evidence without issuing a merge. |
| `merge` | Refresh skills and remote state, recheck the gate and request a head-fenced merge. |
| `protocol`, `validation`, `host` | Public wire records, shared identifier/path constraints and bounded commands/operation IDs. |

Imports are explicit. Remote decoding depends on records, not orchestration.
The merge gate does not invoke GitHub or install packages. Installation does not
depend on PR creation or merge orchestration.

## Shared lifecycle and strict boundaries

`InstallReceipt.phase` now uses `contracts::InstallationPhase`. This intentionally
changes the Rust field type from `String`; the five serialized values remain
`staged`, `previous_backed_up`, `activated`, `verified` and `rolled_back`. The
installation lifecycle stays separate from `ActivationPhase`. TypeScript is
generated from Rust through Bazel, with presentation labels only in locales.

Known journal phases retain their original transitions and replay decisions.
Unknown or malformed phases now fail deserialization before recovery changes any
file. The receipt bytes remain intact for reconciliation. They cannot be treated
as an unfinished operation that is safe to roll back automatically.

PR receipt decoding now requires a known shared `PullRequestState`. Valid remote
values retain their public string representation. Unknown/missing states fail
before PR reuse or merge settlement can return a receipt. The GraphQL decoder
also distinguishes an explicit null aggregate review decision from a missing,
empty or malformed field. Only explicit null retains the no-required-review
interpretation; unknown nonempty decisions still fail the existing merge gate.
The fallback for an unknown individual review uses the review domain's constant,
preserving the same wire value.

## Semantic review

- Release validation retains every required artifact hash and exact source,
  prompt, policy, SuperPOD, upstream, skills and independent evaluator binding.
  Historical missing pins remain readable but cannot pass a new release gate.
- Installation validates evidence and current upstream/skills before recovery
  or staging. It retains directory confinement, unique skill-name checks, binary
  and package hashes, permission-mode hashing and the existing lock scope.
- The staged receipt is durable before activation. Current skills and upstream
  are checked again before moving the previous version. Each rename/journal
  sequence is unchanged; the activated package is smoke-tested and rehashed,
  then upstream is checked again before recording `Verified`.
- Recovery continues to reconcile actual hashes when a crash splits a rename
  and journal update. Explicit rollback refuses to overwrite a later modified
  installation or changed backup. A just-started failed smoke run retains its
  separate recovery path under the still-held lock. Rejected files are preserved.
- PR lookup still rejects truncated/ambiguous matches, closed candidates and
  mismatched remote heads. Creation retains private body-file handling and the
  final head check. Loading a request causes no network or installation action.
- Merge evaluation retains repository/default-branch bindings, exact PR head,
  complete pagination, open/non-draft/mergeable/clean state, successful nonempty
  CI checks and unresolved-thread/approval constraints. After skill verification,
  orchestration fetches another snapshot and checks upstream immediately before
  the existing `--match-head-commit` merge. The returned receipt records actual
  observed state; queue admission is not reported as completed merge.

## Evidence and limits

The extraction audit compares all 38 original definitions/implementations and
functions. It expands remote snapshot decoding, normalizes private visibility,
formatting and equivalent known-phase comparisons, and identifies the stricter
invalid-input paths separately. All nine original delivery/contracts test bodies
are preserved after equivalent typed-phase access normalization.

Seven added regressions cover legacy receipt bytes, distinct lifecycle values,
unknown journal phases without file mutation, a restored-but-unsettled rollback,
CI status interpretation, truncated/mismatched GraphQL observations, malformed
approval decisions and unknown PR receipt states. Focused tests pass 14 delivery
and 2 contracts tests; locale integrity and generated-contract drift checks pass.

The semantic review and behavior tests supplement the structural audit. Full
Bazel checks additionally cover all callers, protocol/locale boundaries, core
startup and packages. Fixtures use local files and explicit fake remotes; they
do not claim live GitHub merge, production installation or deployment success.

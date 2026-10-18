# Prompt evolution model boundary review

Baseline: `65bc6d3dae3d4eb4052d7d1c8461fdfb8cfbf16d`.
This is local implementation and semantic review, not independent signed approval.
Delivery requires repository tests and full snapshot-bound Qualitygate.

## Ownership

| Module | Responsibility |
| --- | --- |
| `registry` | Immutable prompt payloads/versions, append-only registration, ancestor validation and portfolio slots. |
| `protocol` | Snapshot bindings, observations, evaluation contexts, metrics, decisions and ablation records. |
| `pairing` | Complete holdout pairing, exact expected bindings, duplicate detection and critical-regression observation. |
| `metrics` | Pure pooled success/cost summaries and the existing efficiency predicate. |
| `evaluation` | Existing advisory promotion and novelty gates over validated evidence. |
| `exploration` | Persistent ten-experiment budget windows and replay prevention. |
| `quarantine` | Content-bound rule proposals with no automatic adoption authority. |
| `holdout` | Canonical path separation, nested-link inspection and optimizer-path authorization. |
| `identity` | Deterministic serialized-payload digests and required-text checks. |
| `lineage` | Existing bounded, development-only clade diagnostics through a read-only registry view. |

The public facade retains model names and evaluation functions. Imports are
explicit. Protocol records depend on shared contracts/identity, pairing on the
protocol, metrics on validated pair data, and gates on these helpers. Registry,
budget and path-isolation owners do not invoke evaluation or delivery. Lineage
cannot mutate the archive through its new accessor. No service, database handle
or mandatory research dependency is added to core collaboration.

## Shared domains and compatibility

`DatasetPartition` and `TrialVariant` now live in `contracts`; the model retains
`DatasetPartition` and `Variant` exports. The CLI's `EvaluationKind` is an alias
of shared `PromptEvaluationKind`. These are separate categorical domains, with
unchanged serialized values: development/validation/holdout, baseline/candidate,
and promotion/novelty. Unknown or malformed values still fail parsing; none is a
fallback success or approval. Other CLI action handlers and receipt gates are
unchanged.

Rust remains the source for generated TypeScript constants/types. Chinese labels
exist only in frontend locale resources. Unicode prompt data is unchanged and
covered by an external JSON compatibility fixture. The two local crates gain
only a `contracts` dependency; Cargo retains every external dependency version.
Bazel regenerates only the three affected input digests in its lockfile.

## Semantic review

- Prompt identities retain field order, parent sorting, failure conditions and
  content hashing. Registration validates retained content and known parents of
  the same task kind, rejects cycles, and never overwrites an identical version.
  Portfolio capacity, distinct-slot membership and task-kind checks are unchanged.
- Evaluation context still requires distinct snapshots, the same formal policy
  and the same research-required SuperPOD baseline. Each observation binds all
  source, prompt, policy, knowledge and configuration fields to its expected side.
  Trial IDs cannot cross families; missing partners, duplicates and non-holdout
  partitions are rejected before summaries or decisions.
- Promotion retains at least three families with at least three pairs per family,
  no critical regression, and either the exact ten-percentage-point success
  threshold or nondecreasing success with a ten-percent median cost improvement.
  Integer success comparison still prevents rounding a small gain upward.
  Missing token observations cannot support token-efficiency improvement.
- Novelty retains at least two held-out families with exactly three pairs each,
  per-family success constraints, no critical regression, and at least one paired
  ablation with three or more trials demonstrating component contribution.
  Ablations retain unique component IDs and families drawn from the primary
  evidence; the same pairing/context checks apply to their snapshots.
- Evidence digests still cover the same ordered context, observations and, for
  novelty, ablations. Approval flags remain derived advisory output. Host receipt
  recomputation and downstream source, skills, publication and installation gates
  remain responsible for actual promotion or external effects.
- Exploration retains 25% initial share, ten-item windows, five-point adjustments,
  10..50% bounds and durable completed-experiment IDs. Replay and invalid retained
  state fail before mutation. Critical regressions still reduce the share even
  when a window contains promotions. Rule proposals remain permanently quarantined.
- Holdout validation retains root-overlap rejection, nested symlink checks,
  visited-directory tracking and rejection of directory links outside an exposed
  root. Optimizer paths must resolve inside declared workspaces and outside the
  holdout. These checks still require actual sandbox filesystem permissions.
- Lineage retains archive/observation/budget bounds, host-frozen bindings,
  development-only evidence, unique descendant accounting and unchanged formulas.
  Pending jobs do not count as successes; reports always deny promotion authority.

## Evidence and limits

The extraction audit compares forty original model/lineage definitions and
implementations, normalizing private visibility and read-only accessor expansion.
It verifies all three shared enum domains and the unchanged CLI outside its enum
alias. All twenty-three existing model, lineage, CLI and contracts tests remain
intact. Lockfile comparison confirms no external dependency version changed and
checks each regenerated Bazel input digest against its source bytes.

Six new regressions cover literal prompt JSON/hash compatibility and Unicode,
all five evidence bindings, knowledge baseline equality, non-holdout partitions,
cross-family trial IDs, rejected budget replay without mutation, symlink cycles
and escapes, and shared wire-domain parsing. Focused tests pass sixteen model,
nine CLI and four contracts tests. Locale integrity and generated drift checks
cover frontend consumers; full repository checks cover runtime callers/packages.

Local model decisions, path fixtures and mock host adapters do not establish a
real candidate's quality, production isolation, external promotion or deployment.

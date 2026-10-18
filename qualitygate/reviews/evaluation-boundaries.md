# Collaboration evaluation boundary review

Baseline: `4001092b893a3f463adbf111a797bf5b298e9d55`.
This is a local implementation and semantic review, not independent signed
approval. Delivery requires repository tests and full snapshot-bound Qualitygate.

## Ownership

The optional collaboration pilot keeps its public `execute` function and all
twelve public record/enum types at their existing crate-root paths. Serialized
fields, ordering, defaults, enum values and digest inputs remain unchanged.
The refactor adds no service, state domain, database handle or knowledge store.

| Module | Responsibility |
| --- | --- |
| `protocol` | Frozen plan and claim/judgment records; task data comes directly from `execution-model`. |
| `planning` | Validate and freeze matched roles, prompts, source bindings and the task DAG. |
| `registration` | Preflight every existing job and register the fixed cohort through runtime APIs. |
| `collection` | Read job and workflow authority, validate reports, and sequence host adjudication and summary. |
| `bindings` | Check the exact job attempt, task, model, source, prompt and committed receipt. |
| `cards` | Decode bounded worker output and restrict references to supplied dependency cards. |
| `citations` | Check regular Git blobs, revisions, file digests, line ranges and exact quotes. |
| `adjudication` | Apply independent grades bound to the exact plan, claim and evidence. |
| `metrics` | Summarize collected calls and final claims without file, Git or workflow access. |
| `usage` | Read bounded worker-writable trace diagnostics; never provide approval authority. |
| `identity`, `artifacts` | Stable identity normalization and immutable private artifact persistence. |

Production imports are explicit. `Job` and `Task` use their defining crate
directly; only registration needs runtime operations. Pure records and summary
code do not call the scheduler. Shared lifecycle enums remain in `contracts`.

## State boundary

The previous collector branched directly on external state strings. The workflow
adapter now exposes `parse_run_state`, returning `Option<WorkflowState>`. Canonical
values use the existing shared parser; the collector's historical `canceled` and
`timed_out` spellings map to `Cancelled` and `Failed`. This does not change the
canonical enum, generated frontend contract or existing strict owner-proof APIs.

Only parsed `Succeeded` proceeds to receipt validation. Failed/cancelled states
retain their failed-call accounting; running, cancelling, unknown, missing and
malformed observations remain incomplete. The report still preserves each raw
string, including the historical aliases, and uses the existing `unknown` fallback
for non-string or missing values. Parsing alone never verifies run identity,
committed output or successful completion.

## Semantic review

- Plan creation retains request validation, current input collection, common
  SuperPOD source checks and immutable replay. Planning and prompt-registry locks
  remain held across the same writes. Historical v1 plans retain their exact
  prompts and digests; v2 and unknown protocol handling are unchanged.
- Registration validates the full plan and every existing job before creating
  any missing job. It refreshes inputs once and preserves original checked-at
  values when validating retained prompt digests. Both arms keep equal roles,
  call/time ceilings, one attempt, no mutable memory and the original dependency
  edges. Registration does not launch workers.
- Collection retains its deadline, job-attempt checks and workflow-committed
  result comparison. Mutable local receipts cannot replace owner authority.
  Frozen model, configuration, prompt and source/SuperPOD bindings stay required.
- Cards can reference only known, supplied dependency claims. Revisions still
  target the worker's own first-round card. Common SuperPOD content citation,
  regular-file checks and bounded knowledge proposals remain required.
- Citation resolution is separate from semantic support. Invalid citations stay
  visible and cannot receive a supported grade. Host grades still require exact
  plan/claim digests, independent bounded evidence, valid revision/counterexample
  relations and same-arm/round duplicate representatives.
- Summary reduction runs after adjudication and retains the original iteration
  order. Initial claims do not contribute final-claim metrics. Literal duplicates,
  unique supported claims and corrected targets remain scoped by arm. Unassessed
  worker proposals do not count as supported findings or effective corrections.
- `complete` continues to mean no incomplete calls, not success or correctness;
  failed calls remain in the planned denominator. `fully_adjudicated` keeps its
  separate requirements, and `promotion_eligible` remains false. Knowledge
  updates remain proposals pending host review, with no external write.
- Usage still uses no-follow/nonblocking reads, a 16 MiB bound and checked token
  arithmetic. Malformed or unreadable traces produce null diagnostics, never
  trusted billing, independent evidence or a host judgment.

## Evidence and limits

The extraction audit performs 40 comparisons over all 39 original evaluation
definitions/functions and the existing workflow implementation. It expands the
action and summary delegations, normalizing private visibility and formatting.
The explicit state-parser change is separately reviewed and tested. All 21
existing tests across the two crates retain their bodies and ignore attributes.

Four new regressions cover canonical/legacy/malformed workflow observations,
collector wire compatibility and fail-closed accounting, arm-scoped final metrics
with unassessed claims, and malformed/oversized/symlinked/overflowing usage traces.
Focused Bazel tests pass 14 evaluation and 9 workflow tests; one existing test in
each crate still requires an explicit real workflow CLI and remains ignored.

Full Bazel verification also covers callers, shared contract generation, locale
boundaries, standalone core startup and packages. Local fixtures do not establish
real model quality, actual billing, independent scientific support, live workflow
compatibility beyond tested versions or authorization to promote an experiment.

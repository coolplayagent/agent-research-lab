# Prompt evolution host adapter boundary review

Baseline: `c172a48a5e65e73e8b58a02b74faf1b0e56d0bff`.
This records local implementation and semantic review, not independent signed approval.
Delivery requires repository tests and full snapshot-bound Qualitygate.

## Ownership and dependencies

| Module | Responsibility |
| --- | --- |
| `dispatch` | Normalize request/output paths and route the nine existing commands. |
| `protocol` | Strict public request records with unchanged field names and types. |
| `receipts` | Canonical evidence, complete decision recomputation and upstream/skills digest binding. |
| `evaluation` | Prepare host context in the original order, run evaluation and emit validated receipts. |
| `candidates` | Register immutable prompts, validate portfolios and quarantine rule proposals. |
| `lineage` | Read registry/study inputs, diagnose and emit a recomputable lineage receipt. |
| `exploration` | Verify a full batch and commit one durable budget update. |
| `promotion` | Verify activation authority and commit a bound stable strategy selection. |
| `selection` | Validate retained history and resolve registered/runtime prompts without writes. |
| `paths` | Host path separation, destination resolution and holdout path diagnostics. |
| `files` | Strict recursive JSON decoding and optional durable output writes. |

The root is a public compatibility facade. Request records depend on existing
shared models, not handlers. Handlers depend on receipt/path/file mechanisms;
those mechanisms do not dispatch commands or select stable strategies. Runtime
selection reads do not depend on promotion execution. Promotion alone writes
stable selections; candidate registration cannot activate them. The shared
`PromptEvaluationKind` remains the source of the `EvaluationKind` alias. There
are no new lifecycle domains, dependencies, service/database handles or mandatory
research requirements for core collaboration.

## Authorization and effect ordering

- Dispatch still canonicalizes the request first, derives its relative-path base,
  then resolves any output before reading a command request. Existing destinations
  reject a final symlink; missing destinations require an existing canonical parent.
  Command strings and unknown-command errors remain unchanged.
- Strict input decoding preserves historical serde defaults and rejects unknown
  nested fields after deserialization. Candidate approval fields cannot disappear
  silently from nested model records. Unknown shared categorical values fail.
- Both evaluation request shapes remain distinct. Their common preparation still
  resolves isolation roots, validates separation, checks host request/context/output
  paths, reads the context, checks upstream/source binding, then canonicalizes and
  validates the hashed skills artifact. Only then are observations ordered and the
  original domain gate evaluated. The helper's expansion matches both old sequences.
- Receipt schema, field ordering, canonical observation/ablation sort keys and
  evidence digest inputs are unchanged. Validation recomputes the complete decision,
  including the bound repository, default branch, commit and skills hash. Historical
  receipts remain readable but lack authority for new promotion/budget effects.
  Research-required source, prompt, policy, configuration and SuperPOD pins remain.
- Exploration acquires its destination lock before reading retained budget state.
  Each receipt is recomputed and bound to current upstream/skills before its evidence
  digest is recorded. Labels do not establish durable identity. The only budget
  write occurs after the entire batch; a later failure discards earlier in-memory
  updates. Replay, duplicate labels and receipt/destination aliases still fail.
- Promotion verifies host paths, complete context equality, approved promotion kind,
  candidate binding, unchanged source/configuration, registry validity, role/task-kind
  compatibility and ancestry before acquiring the selection lock. It validates
  retained selections and verifies live upstream/skills before checking idempotency.
  A different current baseline or policy rejects activation. History remains unique
  and bounded to five; new selections are validated, skills and upstream rechecked,
  then written. No successful shortcut bypasses freshness verification.
- Read-only runtime resolution rejects an invalid whole selection archive before
  answering any role, and explicit prompt resolution requires a registered matching
  role. Missing selection storage still means no selection. Registry aliases for
  read-only consumers retain existing semantics; host outputs cannot overwrite
  the protected request/context/registry/receipt inputs in their respective gates.
- Portfolio validation still reports no activation, quarantined proposals report
  no adoption, and holdout diagnostics explicitly report no OS isolation proof.

## Evidence and limits

The extraction audit compares 36 original definitions/implementations, all nine
command arms, the entire dispatcher, every public export and five shared test
fixture bodies. It normalizes private visibility, formatting, borrowed handler
input paths and the explicit equivalent PathBuf comparison, and expands the common
context helper. All nine original test bodies are retained in sibling owner files.
Cargo/Bazel declarations and lockfiles are unchanged.

Seven added regressions cover deterministic request ordering and protected inputs;
noncanonical or rebound receipts; nested authority injection and unknown categories;
path aliases into exposed stores; invalid retained selections and role mismatch;
a later failed batch receipt with absent or existing budget; and repeated selection
with stale upstream, changed release or changed installed runtime bytes. Sixteen
focused CLI tests pass. Full repository verification also covers public consumers,
wire/locale drift, packaging and the research gates.

These are local source, deterministic fixture and configured-gate checks. They do
not establish production filesystem isolation, candidate quality, a live remote
promotion or deployment. Path checks continue to require actual OS permissions;
no authorization guarantee is strengthened merely by moving code between modules.

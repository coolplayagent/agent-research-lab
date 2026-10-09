# Evolution request examples

These JSON files contain synthetic fixtures, not measured research results. They
round-trip through the real Rust request types in `evolution_cli` tests.

The host adapter accepts an action, an input JSON file and an optional explicit
output file. Paths inside JSON resolve relative to its input file. Output paths
resolve relative to the invoking shell; their parent directories must exist.

| Action | Input | Output behavior |
| --- | --- | --- |
| `prompt-register` | `PromptRegistration` | Required registry path; loads and validates existing immutable versions before appending. Repeating identical content is idempotent. |
| `prompt-portfolio` | `PortfolioValidation` | Validates registered versions and the 1 stable / 2 specialized / 2 exploratory limits; never activates a strategy. |
| `evaluate` | `PromotionRequest` | Computes the promotion gate. Optional output is a full private evaluation receipt. |
| `novelty` | `NoveltyRequest` | Computes the held-out transfer and ablation gate; same private receipt behavior. |
| `exploration` | `ExplorationUpdate` | Required raw `ExplorationBudget` state file; updates only from recomputable evaluation receipts. |
| `rule-proposal` | `RuleProposalRequest` | Produces a quarantined proposal; never adopts formal policy. |
| `promote` | `StrategyPromotionRequest` | Required selections file; recomputes the approved receipt, verifies current host context and lineage, then selects the candidate for future tasks. |
| `holdout-validate` | `HoldoutIsolation` | Checks canonical path separation and symlink leakage; does not attest OS isolation. |

Copy the gate examples and `context.json` into a private host-controlled directory
and replace all fixture identifiers and measurements with actual evidence. The
example layout expects sibling `holdout`, `optimizer`, `superpod` and `memory`
directories. In deployed lab runs, private held-out tasks belong under the
controller's `state_dir/private` and the process sandbox must hide that state from
agents. Gate input, expected context, receipts and budget files must remain outside
optimizer workspaces and shared knowledge/memory directories.

`context.json` is separately pinned by the host. Every observation must match its
expected baseline or candidate binding. Unknown fields, including supplied
`approved` flags, are errors. Receipts retain the context and observations so their
decision can be recomputed. Missing evidence is a failure, never an approval.

New evaluation requests also require the captured `upstream` repository, default
branch and commit, plus a `skills_manifest` reference containing the exact host
artifact path and SHA-256. Its baseline source must equal that captured default
commit. Copy the complete verified skill manifest into host evidence storage;
replace the example's path/hash placeholders with real installed tree/runtime
digests and release identities. Production manifests include all seven tools.
Do not point evaluation receipts at a mutable global manifest.

Version-two receipts bind these upstream and skills identities into their evidence
digest. Before promotion or an exploration-budget update, the host verifies the
current GitHub default and latest skill releases and rehashes installed files.
An advanced branch, newer skill release, changed artifact, or unavailable network
requires fresh evaluation; two matching but old host files cannot approve a
candidate. Fixed paired trials retain their original inputs while running.
Historical unbound receipts remain readable, but cannot authorize new gates.

Register `prompt-register.json` first, then replace the stable version placeholder
in `prompt-portfolio.json` with the returned SHA-256. To propose a descendant, add
the parent's version to `definition.parent_versions` and change its content and
reason. Runtime callers resolve explicit versions using
`resolve_registered_prompt(registry_file, version, role)`; registration alone
does not select a prompt for execution.

After independent evaluation succeeds, `promote.json` selects its exact registered
candidate for a role. For runtime use, the registry is
`state_dir/evolution/prompts.json` and promotion output is
`state_dir/evolution/selections.json`. A candidate must descend from its evaluated
baseline; after the first promotion that baseline must still be the current
selection. Approval cannot be reused for a changed prompt, source, model/tool/
memory configuration, formal policy, or expected host context. Repeating the same
promotion is idempotent. At most five previous stable versions are retained per
role for traceability; automatic rule adoption remains unavailable.

New tasks use `resolve_selection(state_dir, role)` at enqueue time and freeze that
version. Existing tasks retain their pinned prompt. No selection uses the initial
role template; corrupt or unknown selections produce an error instead of silently
falling back.

The runtime reads the raw budget at `state_dir/evolution/exploration.json`.
Exploration deduplicates by the evaluation's evidence digest, not its human-facing
label; changing a label or observation order cannot count the same experiment
again. A full batch is validated before its state file is replaced. Novelty
evidence alone does not count as formal prompt promotion.

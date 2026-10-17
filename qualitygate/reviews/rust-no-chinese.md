# Rust source character rule review

This is an agent-authored source-mapping review by Codex on 2026-10-11,
under the user request to record and enforce programming constraints.
It is a local policy candidate for repository review, not independent approval
or signed acceptance. The bound record is in `qualitygate.yaml`.

The reviewed source is the complete `API and presentation boundaries` section
of `AGENTS.md`. The rule enforces its explicit prohibition of Chinese characters
in Rust source, including comments and tests. `files`, `entity: file`,
`change: all`, `**/*.rs` and `\p{Han}` cover the entire Rust file inventory,
not only added lines. The source hash and review binding were obtained from the
published Qualitygate 0.5.7 CLI, not inferred or manually constructed.

The mapping deliberately covers only this textual obligation. API ownership,
server semantics, locale completeness, preservation of user Unicode, and
research isolation require the boundary, locale, storage and integration tests
plus architectural review. Han matching also rejects characters shared with
other languages; multilingual source fixtures belong outside Rust files.
Escaped Unicode and runtime-generated text are outside this regex guarantee.

The published CLI was exercised in an isolated Git fixture using the unchanged
source section and rule. An English Rust comment passed (exit 0); a Han comment
failed with `rust-no-chinese` (exit 1); deleting the bound source review produced
an incomplete gate (exit 2). The fixture policy isolated this rule without
changing repository checks. Full repository checks remain mandatory.

## Optional knowledge binding review on 2026-10-17

The user explicitly made SuperPOD an optional external knowledge binding.
AGENTS.md now records that scope and preserves existing research evidence gates.
Codex reread the complete API and presentation boundaries section: its bytes and
textual obligation are unchanged, as confirmed by `qualitygate rules source`
0.5.9 (`sha256:542c1a2ad26f46b8977ff0f8528221f2020af523c35f5fd29670a752ed58bde6`).
The existing rule definition, inventory scope, severity, requirements and full
checks remain unchanged. The source-review binding in qualitygate.yaml was
renewed using the digest returned by this runtime's doctor projection. This is
a local agent-authored source review, not independent approval or signed
acceptance. Outcome semantic ownership is reviewed separately in
`outcome-collaboration.md` and exercised by storage, HTTP and service tests.

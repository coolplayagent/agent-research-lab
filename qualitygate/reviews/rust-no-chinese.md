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

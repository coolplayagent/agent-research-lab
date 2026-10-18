# Knowledge publication boundary review

Baseline: `717e8a70f52227e9755e57c11bfed79b553271dc`.
This records local implementation and semantic review, not independent signed
approval. Delivery requires repository tests and full snapshot-bound Qualitygate.

## Ownership

The public crate facade retains the report, preparation, publication and refresh
entry points, records and historical `ResearchReport` alias. Private modules
introduce no storage service, shared database handle or mandatory core dependency.

| Module | Responsibility |
| --- | --- |
| `report` | Report payloads, stable Markdown rendering and rendered-payload digest. |
| `validation` | Content-review attestation, evidence pins, citations and confined source archives. |
| `protocol` | Explicit repository layout and publication/index request and receipt records. |
| `preparation` | Validate and write a contribution on its isolated outcome branch. |
| `snapshot` | Validate ancestry and the complete reviewed manifest, including deletions. |
| `publication` | Recheck the worktree, commit allowed changes, verify committed bytes and push the exact head. |
| `host` | Bounded Git and relay-knowledge calls, including failed/incomplete map validation rejection. |
| `index_refresh` | Inspect readiness and active maintenance, submit at most one update and return actual status. |

Imports are explicit. Rendering does not execute commands; snapshot verification
does not publish; index refresh does not create contributions. Each owner's unit
tests are in its sibling test file, with shared local fixtures in `test_support`.

## Shared domains and compatibility

`FindingKind` moves to `contracts` and is re-exported at its existing Rust path.
Its four serialized values are unchanged. `SourceReference.access_status` now
uses `contracts::SourceAccessStatus`, intentionally changing that Rust field from
`String`. Its five serialized values exactly match the previous validator's
allowlist. Source availability is separate from index content integrity and from
finding classification. Unknown or malformed access values now fail report
input decoding; there is no fallback that implies downloaded or verified content.
Previously valid reports retain their serialization. No persisted file migration
or rewriting is introduced.

The existing `superpod_commit` and `superpod_root` serialization names and their
knowledge aliases remain intact. Frontend constants/types are generated from
Rust through Bazel. Chinese labels belong to frontend locale resources; Unicode
user content is preserved verbatim. Multilingual compatibility input and expected
Markdown live in external test fixtures, not Rust source.

## Semantic review

- Validation retains the caller's explicit publication-review attestation, all
  source/prompt/policy/external-knowledge pins, reserved-ID protection, public
  repository constraints, content tripwires, citation completeness, measured
  evidence requirements and source digest/confinement checks. The tripwire is
  still not an automatic declassification claim.
- Preparation validates both the source checkout and the pinned isolated
  worktree. Reuse requires the same repository, branch and exact baseline; it
  cannot overwrite a differing existing report. Maps are validated before and
  after routing changes, and an existing source ID must retain its route.
- The review manifest covers every changed/untracked/staged allowed contribution
  path and explicit deletion. Unlisted paths, symlink aliases and non-regular
  committed tree entries remain rejected. The report hash must be included.
- Publication checks its dedicated branch, distinct base, baseline ancestry,
  complete worktree manifest and configured origin before external effects.
  Renames still require explicit review. Committed bytes must equal the reviewed
  manifest before the exact commit is pushed. PR creation still requires the
  pushed head through the delivery adapter; preparation alone authorizes no PR.
- Index refresh preserves raw status responses, exact commit matching, explicit
  `stale=false` and complete integrity evidence. A recognized integrity state
  takes precedence over the legacy boolean field; unknown or malformed states
  cannot be rescued by `complete=true`. Active work or maintenance suppresses a
  duplicate update, and a refresh submits at most one update without polling.
- The adapter continues to operate only on explicitly bound external repositories.
  No core startup, research evidence gate, host acceptance rule or queue replay
  transition is weakened by these module changes.

## Evidence and limits

The extraction audit compares all 26 moved original definitions/implementations
and functions, expanding the extracted index-observation helpers and normalizing
visibility, formatting and equivalent typed access-state operations. It separately
checks the shared enum wire values. All 12 existing publication, contracts and
outbox test bodies are preserved after typed fixture initialization normalization.

Seven new regressions cover exact legacy JSON/Markdown/digest compatibility and
Unicode data, unknown access values, verified-archive checksums, reviewed deletions,
symlink rejection, unknown/stale index observations and distinct shared domains.
Focused tests pass 13 publication, 3 outbox and 3 contracts tests. Generated drift,
locale integrity and full repository checks cover the frontend/shared consumers.

This review and the behavioral tests supplement the structural audit. The local
Git fixtures and explicit fake CLI do not establish live external publication,
GitHub PR creation or production deployment success.

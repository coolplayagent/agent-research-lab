# Dependency cache boundary review

Baseline: `8685cce549bb020c361d4242c0b29bb35891f964`.
This records local implementation and semantic review, not independent signed
approval. Delivery requires repository tests and full snapshot-bound Qualitygate.

## Ownership and dependency direction

| Module | Responsibility |
| --- | --- |
| `preparation` | Public entry points, frozen input digest, host cache selection and the final performance marker. |
| `report` | Existing metric serialization and the resumable-deadline error type. |
| `cargo` | Locked public crates.io archive and sparse index selection; fixed public index configuration. |
| `bazel` | Lock-reachable registry/extension/archive hashes and pinned Bazelisk binary selection. |
| `copying` | One preparation clock and retained-byte/file ledger; bounded hashing, temporary copies and replacement. |
| `metadata` | Literal integrity decoding, verified source archives and bounded named tar-member reads. |
| `rust_toolchain`, `python_toolchain` | Host-specific, literal toolchain dependency selection. |
| `files` | Absolute private directory traversal and bounded no-follow regular-file reads. |

The facade retains public function/type paths. The report module depends on no
execution module. Preparation composes Cargo/Bazel selectors and a single copy
session; selectors share copy/file/metadata mechanisms. Toolchain selectors read
verified metadata without invoking Cargo, Bazel or candidate Starlark. This
separates data, orchestration and effects without adding a module dependency cycle.
Production imports are explicit; unit tests live in sibling test files.

## Semantic review

- Preparation still validates worktree/log directories before beginning. Its
  timer starts before reading the input digest, just as in the original struct
  initializer; extracting construction must not reset that deadline afterward.
  Finish records elapsed time before the same final marker write.
- The input digest retains exactly the five named lock/toolchain/version files,
  their order, name framing and byte limits. Existing metric fields and schema
  semantics are unchanged. Readiness binds these inputs and schema version only;
  it does not certify mutable private cache contents, evaluation results or
  permission to execute an agent effect.
- Cargo still selects only public crates.io package sources and approved registry
  directory names. Archive identity and checksums are checked; sparse index paths
  are derived from validated package names. Index configuration contains fixed
  public endpoints, never copied host configuration or credentials.
- Bazel still visits only the allowed user-cache layout and hashes reachable from
  the task's frozen locks. Unrelated host CAS contents are not enumerated or
  charged against the task budget. Public registry metadata bytes must match the
  pinned digest before recursive dependency metadata is interpreted.
- Metadata parsing retains the same strict SHA-256 encoding/padding, collection
  bounds and literal-only declarations. Archive bytes are verified before named
  tar members are read with the existing ten-second command limit. No candidate
  Starlark is executed. Rust defaults/pins, nightly formatter and exact host
  triples, and the Python default minor mapping remain unchanged.
- The same copy session accounts for both adapters. Existing copies are verified
  and still consume retained-file/byte budgets; reuse does not bypass limits.
  Deadline checks remain inside copy/hash loops with the same 55-second local
  bound and enclosing process deadline. Files that grow or shrink fail checks.
- Source and destination ancestors still reject symlinks. Regular inputs use
  no-follow/nonblocking opens, while temporary outputs use exclusive no-follow
  creation. Copies retain private permissions, checksum checks before rename,
  and failure cleanup. Existing destination bytes survive a failed replacement.
- The ready marker is written only after both adapters finish successfully.
  Failed/deferred preparation retains resumable private copies, never shared
  writable cache handles. No gate, evidence pin or promotion rule is changed.

## Evidence and limits

The extraction audit compares all 31 original definitions/functions and seeder
methods, including private visibility normalization and expansion of construction
and finish. All prior constants, archive endpoints and sequence generation remain
identical. All eleven original test bodies are retained, apart from correcting
the opt-in host proof's obsolete `app/Cargo.toml` fixture path to `apps/cli/Cargo.toml`.
Its existing environment-dependent ignore condition is unchanged.

Five new regressions cover failed-replacement preservation/cleanup, private file
modes and independent inodes, all marker inputs and unknown marker schemas,
checksum rejection before metadata extraction, and literal host Rust toolchain
selection from a locally generated tar archive. The focused suite passes fifteen
tests with the one pre-existing host-cache/bubblewrap proof ignored.

Full repository verification also checks downstream runtime callers, core/package
boundaries and locale/generated contracts. Local fixtures establish these bounded
behaviors; the ignored host proof is not evidence of an actual offline build in a
production agent namespace. Cached artifacts remain performance inputs only.

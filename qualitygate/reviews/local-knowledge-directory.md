# Host-managed knowledge and directory snapshot review

This is an agent-authored source review, not independent approval.

## Ownership and retired connections

The registry still owns only service configuration and default bindings. The
packaged knowledge service owns target configuration and publication receipts;
knowledge bytes remain in the selected Git or directory target. Registry startup
validates the old configuration, backs it up privately, derives the removal of
external knowledge connections and their group/goal/topic defaults, validates the
new configuration, and atomically commits it. A revision increment fences clients.
A backup and summary may exist after a failed migration; the configuration commit
is the authority. Reopening a migrated configuration does not repeat migration.
Verifier bindings, local service configuration, historical outcomes and receipts
are not rewritten. New external knowledge settings and proxy access are rejected;
other external execution and verifier services are unaffected. The local knowledge
service uses the same versioned IPC client rather than shared mutable state.

## Directory semantics and evidence

The directory adapter remains independent of Git executables and the Git adapter.
It compiles only descriptor-read `.gitignore` files within the chosen root, never
machine-global ignore configuration. `.git` and `.outcomehub` are excluded before
traversal. Nested overrides apply to traversed directories; excluded parents stay
excluded unless included by applicable parent rules. Rules themselves participate
in the revision, even when a rule ignores its own file. Direct reads and future
publication destinations check the same policy. Receipt lookup still uses the
existing durable record and content check, preserving already applied writes.

Each regular file is streamed through SHA-256 using a fixed buffer. File identity,
size, nanosecond timestamps and linkage are checked around hashing; two snapshots
must agree around inspection. No metadata-only cache substitutes for content
hashes. Symlinks and special files are rejected, directory descriptors anchor
traversal and text reads retain their bounded payload. A shared per-operation
budget accounts for repeat scan passes and text bytes, with fixed entry, byte,
depth and wall-clock bounds. Ignore input and parsing are additionally bounded.
Hash optimization changes throughput, not digest values or acceptance policy.

The `directory-v2:` domain distinguishes the new selection policy. Old expected
revisions fail, requiring preview; pending/recovered publication still enforces
request identity, host acceptance and the existing target/configuration/outcome
pins. Historical applied receipts are preserved rather than silently upgraded.
Public errors carry shared typed codes and parameters through private service
replies to HTTP and frontend locale lookup. Transport text is not shown as UI.

## Evidence

Regression coverage includes large attachments, more than 4096 files, nested
ignore overrides, blocked direct reads/publications, entry/byte/depth/time limits,
concurrent modification, symlink/special-file rejection, old-revision rejection,
legacy configuration migration, settings rejection, locale interpolation, wire
compatibility and packaged local directory/Git behavior. Real SuperPOD validation
runs with the source filesystem mounted read-only and isolated service state.
The final full Qualitygate report remains the authority for executed checks.

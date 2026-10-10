# Pre-split serialization fixtures

These fixtures were captured by executing the actual library at commit
`abe6720677fa007c8bc875144de6eb0a1c190e9f`, before the crate split. They are
independent expected results, not values calculated by the implementation under
test. `provenance.json` binds the old source tree, generator, dependency lockfiles
and generated JSON files.

All paths, commits, messages, process IDs and credentials **names** are synthetic.
No credentials, production task data or model calls were used. These samples test
JSON representation and identity hashing; they are not runnable workflow grants
or evidence that the synthetic research inputs passed freshness validation.

For both legacy and fully populated records, `*.input.json` is the supplied JSON
and `*.serialized.json` contains the exact compact bytes emitted by the old
library. Preserve the latter byte-for-byte, including field order and absence of
a trailing newline. The legacy Job/Launch samples include unknown historical
metadata to preserve their permissive decoding behavior. Config and Task retain
their existing strict schema. `digests.json` records config, task, job, launch,
cohort and member identities emitted by the same old library.

`generate.rs` is the generator actually executed. To reproduce, create an
isolated Cargo package with `serde`, `serde_json`, and a `lab_old` dependency whose
package is `agent-research-lab` and whose path points to a clean checkout of the
exact commit above. Copy its Cargo.lock to the isolated package, use the generator
as `src/main.rs`, and run it offline with a new output directory as its sole
argument. Do not regenerate compatibility expectations from the current crates.

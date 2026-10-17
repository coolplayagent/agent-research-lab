# Protocol compatibility fixture

`wire-4fd42de.json` is synthetic test data captured by executing the original Rust
serialization code at commit `4fd42de092b8af96913c7df64de0c0b82e9123d0` before the
module extraction. It is not generated from the new implementation.

The capture deserialized each `input` into the original type, then stored
`serde_json::to_string` as `wire`. Types came from `crystal` (message, group, goal,
coordination, outcome, iteration and lineage), `im_storage::Request`, and
`service_api` (agent input and default service configuration). The publication
input is the repository's existing `examples/outcome-publication.json`; the
configuration was produced with `Configuration::defaults(Path::new("/example/state"))`.
All identities, paths, tokens and content are synthetic; there are no credentials
or experiment records in the fixture.

To recapture for an intentional protocol migration, check out the recorded commit
in an isolated worktree, deserialize the frozen inputs into those original types,
and serialize them using that commit's dependencies. Review changes to byte
order, omitted/default fields and tags explicitly. Do not update expected bytes
from the new code merely to make a compatibility test pass.

# Native output byte contract review

This is an author review, not independent approval. After the runtime file fix,
an isolated real research worker reached its final response but the bridge rejected
oversized evidence. The native prompt and schema did not describe the limits that
the adapter already enforced.

The existing 8,000-byte summary and 8,192-byte evidence limits now have shared
constants owned by the service API. The native schema descriptions and prompt
advertise those same UTF-8 byte budgets, recommend compact evidence rather than
duplicating the deliverable and request a byte-length check before submission.
Required nested coordinator fields and artifact references must still be preserved.
No JSON Schema character-count constraint is substituted for a byte limit.

The bridge and service validators retain their existing limits and failure behavior;
there is no truncation, automatic success or relaxed evidence validation. An
oversized native diagnostic reports only the byte count and limit, not its content.
API shapes, state machines, identities, receipts and host acceptance remain intact.

Regression coverage accepts exact byte boundaries with external multilingual test
data, rejects one extra byte, preserves nested proposals and user input, and checks
that the prompt advertises the enforced limits. Real native execution, full Bazel
tests and Qualitygate remain necessary deployment evidence.

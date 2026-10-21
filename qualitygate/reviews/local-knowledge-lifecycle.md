# Local knowledge lifecycle review

This is an agent-authored implementation review, not independent approval.

The collaboration host owns only service registration and child lifetime. A separate,
replaceable knowledge process owns targets.json, target revisions and publication
receipts. Contents remain in user-selected repositories or directories; neither the
registry nor the HTTP layer opens the gateway's files or collaboration database.

Local registration is explicit and optional. The core starts without the sibling
binary. Existing outcome services deserialize as external and serialize without the
new default field, preserving their wire shape and service digest. External services
are never adopted, and management roles cannot be changed in place. Managed endpoint
validation confines IPC to private host state. Disabling a service or shutting down
the host drops owned bounded processes; failed target addition creates no target or
binding. A prepared empty gateway may remain configured and can be retried.

Inspect and Add use the same versioned knowledge protocol as existing requests. Add
is serialized by the gateway's operation lock, refuses conflicting identifiers, and
is idempotent for equal targets. Configuration Save retains revision checks. A root
content scope is accepted only in target configuration, while document paths retain
all traversal and metadata exclusions. Git inspection never checks out, creates a
branch or commits. Read-write addition requires an explicit existing branch that is
not checked out. Publication still requires accepted outcomes, explicit preview and
confirmation, pinned baselines, compare-and-swap and durable receipts.

Evidence: service-api legacy wire regression; service-registry private endpoint
regression; knowledge-service root-scope, duplicate, failed-add and prepared-branch
regressions; local-knowledge-smoke exercises the real package and RPC boundaries,
optional-runtime failure, read-only reads, traversal rejection, persistence and
owned-process cleanup. Frontend regression and browser checks cover direct Git and
directory forms. Bazel dependency-boundary tests remain enabled.

Only generic presentation loses the SuperPOD special case. Persisted legacy field
names and research adapter pins and acceptance gates remain unchanged. New generic
knowledge registration creates no research evidence or promotion.

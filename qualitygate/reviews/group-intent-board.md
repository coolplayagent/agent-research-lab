# Group intent routing and availability review

This is an implementation-author source review, not an independent approval or a policy change.

## Ownership and admission

Operator `send` enters a storage command. The storage writer appends the human message,
creates its durable routing task and publishes its acknowledgement event in one transaction.
Announcement/private/temporary conversations retain ordinary messaging. Worker bearer
publishes and host-authored task events do not enter intake. Request IDs deduplicate retries;
pre-upgrade messages are acknowledged without replaying them into a new execution.

Coordinator proposals remain untrusted. The existing lease, graph, membership, budget and
attempt checks still gate their application. Attach validates conversation ownership and the
last human-edit revision, supplements/returns its target and completes the routing record
in one transaction. Execution-only ticks may advance revision without invalidating human
intent. A reply to an unfinished intent cannot create a duplicate plan. Concurrent human
edits, terminal targets and invalid cross-group selections fail without partial writes.

## Lifecycles and projections

MessageProcessingState is distinct from TaskState and from artifact verification/acceptance.
A direct coordinator reply has no delivery and cannot count as human acceptance. Plan
converts the intake record into an outcome task, retaining its original message association.
Completed routing records retain their selected target. Event projections carry host-authored
metadata in the existing message event column; legacy GoalEvent decoding and wire fields
remain supported. No message or artifact text is used to infer lifecycle state.

Presence displays execution availability from the maximum valid node/legacy-work lease
across groups. Expired leases never imply busy. Transport delivery preferences retain a
separate accessor and the embedded/remote subscription contract remains unchanged.
All reads and writes to production collaboration data still belong to the storage service.

## Verification and limits

Storage/API tests exercise durable intake, retry deduplication, restart, clarification,
reply association, cross-group denial, human-edit conflict, lifecycle separation and
concurrent availability. Existing DAG, acceptance, result-fencing and embedded/remote
transport tests continue to apply. Frontend behavior tests cover scoped queries, stale
responses, group links and reply controls. Full Bazel and Qualitygate runs, a real adapter
scenario and browser checks are required before deployment.

Routing relevance depends on the coordinator model; ambiguous inputs must be clarified.
Candidate context is bounded and explicitly reports older omitted tasks. Online denotes
idle availability, not verified configuration. Deploy core and storage binaries together;
new task kinds are not readable by older binaries, so rollback requires the stopped-store
backup. Existing JSON records use compatible defaults; schema 10 tables are unchanged.

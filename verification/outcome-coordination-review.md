# Outcome coordination ownership review

This is an author-owned architecture and state-machine review, not independent approval. The user explicitly authorized replacing topic coordination with outcome tasks and deleting topic-associated history. That authorization supersedes the retired topic retention wording.

## Authority

The collaboration storage writer alone persists tasks, nodes, graph versions, attempts and outcomes. HTTP task management accepts only human operations. Coordinator claims and proposals are host-only storage commands. All workers and the coordinator use the same existing execution protocol and receipt checks. No new storage connection or vendor-specific execution path was introduced.

A submitted artifact is different from coordinator checking, human task acceptance and independent evidence verification. Task acceptance cannot create outcome verification/acceptance receipts. Existing research and external knowledge gates remain in their original owners.

## State transitions

A new task enters planning; missing inputs enter waiting_input. A valid, membership-scoped acyclic graph enters running. Only pending nodes whose dependencies are approved with persisted artifacts can be claimed. Every result binds the claim, graph revision, deadline and attempt. Checking a submission enables dependents. All required nodes must be approved before a final delivery enters review. Only the operator can complete it. Returning requires a revised graph before another delivery.

Revisions retain attempted-node history, immutable artifacts and previous graph definitions. Changed or failed nodes and their descendants cannot reuse old approvals. Node count, attempts, graph revisions and elapsed active time remain cumulative. Waiting for human input does not count as active time. Cancellation invalidates leases; the runtime drops execution calls, and existing executor/sandbox disconnect handling bounds and terminates the corresponding process.

## Migration and compatibility

Schema 10 is offline and copy-based. It deletes topic/subject registries and topic-associated goals, outcomes and dedicated evidence. The old topic listing associated goals through the conversation; retirement follows that same relationship, also collecting explicit outcome paths. Unrelated goals and ordinary messages remain byte-checked. Removed task system messages become tombstones while sequence/reply/cursor identity survives. External knowledge content is not erased.

Outcome v2 uses group ownership. Retired topic operations are unavailable; old outcome endpoints return a protocol upgrade response. The retained v1 wire fixture continues checking unaffected messages, goals, agent inputs, configuration defaults and storage operations. Topic and outcome-v1 fixture entries are retired explicitly with those interfaces. New task and outcome behavior has separate regression coverage.

Bundled local knowledge configurations upgrade their protocol with a private backup and revision increment. The coordinator selects a verified configuration once, records its digest, and probes its structured protocol before use. Changing the selected adapter/model clears protocol evidence.

The native provider bridge previously constrained evidence to an empty object. Native CLI output now encodes the complete evidence object as `evidence_json`, then validates and decodes it to the unchanged AgentOutput protocol. Tests reject malformed/non-object evidence and preserve nested coordinator proposals. All four provider fixtures exercise this boundary. Deployment must refresh bindings onto the new bridge and execute their real validation; old bridge paths cannot prove coordinator compatibility.

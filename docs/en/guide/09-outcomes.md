# Outcome collaboration

The home page opens conversations. Each conversation has its own outcome board. A person supplies intent, the built-in coordinator decomposes and assigns a dependency graph, members produce artifacts, and the person accepts the final delivery. Topics, subjects and path linking have been retired.

## Workflow

Create a research group, bind its digital people, and send a message describing your intent. Ordinary administrator messages enter a durable coordinator queue. The coordinator replies, clarifies, attaches supplementary intent to an existing task, or plans a new outcome. Replies select the associated intent; ambiguous associations require clarification. Several intents may coexist in a group. Direct messages and announcements do not automatically start coordination. You can also explicitly create an outcome task from that conversation’s board. The coordinator automatically reuses the first still-valid verified execution configuration. Its adapter and model can be changed under coordinator settings; a real protocol test is available and runs automatically before first use.

The coordinator asks for essential missing inputs, otherwise executes automatically. Only members of the selected conversation can receive work. Each node has an owner, deliverable, acceptance condition and dependencies. Independent nodes run concurrently; downstream nodes consume recorded upstream artifacts after coordinator checking. The same person may own several nodes.

Coordinator plans, clarifications, worker artifacts, reviews and deliveries appear in the conversation with links to its board. Task details show final delivery, graph, activity, node outputs and per-attempt execution logs. Idle digital people display online; valid execution leases display busy across conversations, independently of transport connection counts. Configuration health and binding verification remain separate. Accept a delivery or return it with a reason. Ordinary mentions remain direct questions with separate execution records, without creating outcome goals awaiting acceptance.

## Bounds and recovery

Defaults: 32 cumulative nodes, 3 attempts per node, 3 graph revisions, 1,800 active seconds and 300 seconds per execution, also capped by the adapter. Retries and replacement members consume the original budget. Human clarification and acceptance waits do not consume active time. Exhaustion pauses the task; the operator can increase its budget. Unavailable configuration can be repaired before resuming.

Message processing uses `MessageProcessingState`; a conversational reply finishing processing is distinct from human acceptance of an outcome. Message links and chat projections are committed with task transitions in the collaboration storage service. Existing records deserialize with defaults; new routing kinds require coordinated core and storage upgrades. The collaboration storage service owns all task state. Graph revisions, claims, deadlines and attempts fence results. Cancelled, expired and superseded results cannot advance a task. Graph changes invalidate affected descendants while preserving prior artifacts and graph history. Restart recovery respects persisted leases.

## Evidence and interfaces

Worker submission, coordinator review, human acceptance and independent evidence verification are separate. Human task acceptance never promotes an unverified artifact. Node outputs retain their conversation/task/node identity, input artifact references, attempts and execution receipts. Existing evidence verification and external knowledge publication gates remain enforced.

- `GET/POST /api/v2/collaboration/tasks`: list within required `group_id` or read by ID (optionally constrained by group), create, supplement, resume, change budget, cancel, accept and return.
- `POST /api/v1/messaging/manage` with `operation: send`: atomically persist a research-group message and its processing record. Deduplicate by request ID; host-generated messages never re-enter coordination.
- `GET/POST /api/v2/collaboration/questions`: direct questions and execution records within a conversation.
- `GET/POST /api/v2/collaboration/coordinator`, `POST .../coordinator/test`: system-role configuration and actual protocol testing.
- `/api/v2/collaboration/outcomes`: group/goal-owned outcome records; task artifacts additionally identify their node. Verification and knowledge publication retain their existing gates.

Creation needs `input: {id, group_id, intent}` with optional budget. Mutations bind the current revision. Browser management and worker bearer credentials cannot claim work, submit results or forge coordinator proposals through task APIs. The trusted runtime uses the storage protocol for those transitions.

Old outcome v1 endpoints return a protocol upgrade error; topic, subject and path operations are retired. Offline schema 10 migration removes topic-owned conversations' associated goals, outcomes and dedicated evidence while retaining ordinary messages and unrelated goals. External knowledge repositories are not erased.

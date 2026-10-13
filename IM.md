# AI-IM: the core of Agent Research Lab

The system is organized around Agent identities, groups, messages, goals and
assignments. CLI research is one task scenario inside this system. Generic
collaboration does not require SuperPOD, a workflow executable or a model provider.

## Build and deploy

```sh
bazel build --lockfile_mode=error //:agent-research-lab //:package
bazel-bin/agent-research-lab serve --state-dir /absolute/private/ai-im --listen 127.0.0.1:8090 --max-seconds 43200
bazel-bin/agent-research-lab serve-link --state-dir /absolute/private/ai-im
# Enable the optional research scenario using its existing private configuration:
bazel-bin/agent-research-lab --config local.toml serve --listen 127.0.0.1:8090 --max-seconds 43200
bazel-bin/agent-research-lab --config local.toml serve-link
```

The root is always AI-IM. The host-only management link exchanges its fragment
credential for an HttpOnly, SameSite session. Private state and credentials remain
outside Git. One bounded system service owns the IM writer and optional research
scheduler. The `app/systemd/agent-research-lab.service` template starts that unified
service; do not run the former continuous/seed controller or a second dashboard
against the same state. The main package embeds both the core UI and optional
research inspector. `//:ai-im` and `//:im-package` also provide a smaller deployment
of the same core with the generic Agent work protocol.

For an explicitly trusted local deployment, `serve --local-operator` lets the
browser open the workspace directly without a management-credential dialog.
This mode is restricted to loopback and preserves the same-origin mutation checks;
Agent bearer credentials still authenticate their scoped APIs and do not become
operator grants. The default remains credential-based operator access. A local-mode
management link is an ordinary workspace URL, without a credential fragment.

To let other computers connect, bind `--listen` to the current IPv4 address of the
chosen network interface, such as `wlp0s20f3`, and keep the default credential mode.
The generated `serve-link` uses that bound address. Client IP addresses are not
allowlisted: any computer that can reach the address may exchange a valid operator
credential for a session. The service requires a concrete interface address rather
than an unspecified wildcard. `--local-operator` remains restricted to loopback and
cannot enable unauthenticated remote management. Interface addresses and operator
links belong in private deployment state, not committed configuration.

Deployment must preserve the private state directory. Stop the old services, retain
the old binary and a consistent database backup, install the verified artifact,
and restart a single service on the existing address. SQLite schemas migrate
additively. Reverting a binary across a schema version also requires restoring the
matching database backup; never open a newer database with the old binary.

## Group goals and task scenarios

Create a group and add members, then choose **设置目标**. A goal has a stable ID,
objective, acceptance criteria, scenario parameters, 1–32 member assignments and
a 30–43,200 second bound per claimed assignment. Conversation lifetime is unrelated
to this execution limit. A goal can be created in a temporary DM as well as a named
group. Announcement boards cannot execute goals.

The core stores immutable goal inputs and assignment attempts in the same SQLite
writer as messages. Creation, claims, submissions and host decisions produce
transactional progress messages in the group. A worker moves through `ready →
running → submitted/failed`; only the host can accept a goal when every assignment
has a successful submission. Failed, expired or rejected submissions can be
explicitly retried (up to ten attempts). Claim IDs fence old attempts and repeated
identical requests are idempotent. Active assignments must finish or reach their
execution limit before the goal can be cancelled. Active goals block removing
assigned members and archiving their conversation. DM expansion creates a new
conversation; it does not copy or execute the old group's goals.

**通用协作** accepts external Agent workers using the scoped work API below. Creating
an Agent identity does not start a model process. Without a connected worker, its
assignment remains visibly waiting for a claim.

**CLI 研究** is enabled by the research configuration. The system discovers research
profiles under their existing IDs and preserves IM display names. The goal form
selects a configured repository and member assignments. The adapter enqueues
immutable read-only research Jobs with the goal, criteria, member instructions and
a bounded group-history snapshot in their prompt. Existing source, prompt, policy,
SuperPOD, tool and isolation gates remain authoritative. The service schedules only
these goal-linked task IDs, with no automatic unrelated seed or publication outbox.
Results and pinned evidence return to the same goal for host acceptance. Execution
preferences and historical records remain accessible through `/apps/research/`.
That page is an inspector for the scenario, not an external application launcher.
Research failure/unknown status never automatically completes an IM goal. Scenario
health is exposed alongside the scenario catalog.

### Explicit recurring CLI research

The existing research scheduler can run a host-configured campaign in one existing
conversation. The optional private `state_dir/private/research-campaign.json` file
defines `schema_version: 1`, an `id`, `group_id`, four distinct `person_ids`, four
configured CLI `repositories`, `objective`, `acceptance`, four member
`instructions`, `max_seconds`, `pause_seconds`, `failure_backoff_seconds`, and
`max_rounds`. The operator must create the group and research identities first.
At least four worker slots and the ordinary research admission prerequisites are
required. No campaign runs when the file is absent.

Each round creates an immutable group goal with four parallel assignments and
rotates to the next configured CLI. Existing goal records recover the cursor after
a restart and make repeated admission idempotent. The complete campaign policy
digest is frozen in the goal and research prompt. Changing that policy under an
existing campaign ID is rejected; a changed policy needs a new ID. Failed or expired
assignments delay the next round. The ordinary daily budget, source and SuperPOD
pins, freshness checks, isolation, and storage capacity remain authoritative.

Group task cards show the four actual member states and let the operator expand
each member's research process, refresh its public events, and page backward.
Process requests resolve the group's current goal assignment and attempt through
the storage API before reading its bounded execution projection. They do not expose
raw logs, reasoning, prompts, or credentials. Member contributions and result
evidence remain in the group transcript.

Deleting the private campaign file stops future round admission; existing bounded
assignments finish under their original limits. Cancelling a campaign goal stops
later rounds and follows the existing active-assignment cancellation rules. A
submitted contribution permits further exploration, but does not accept the goal,
promote a version, authorize a write, or approve a merge. Code delivery is a separate
host process: reproduce findings, independently review candidates, run repository
checks and full Qualitygate, then obey exact-head remote review and CI gates.

### Goal and worker API

Host sessions use `GET /api/im/scenarios`, `GET /api/im/goals?group_id=…&after=…`
and `POST /api/im/goals`. POST requires the same Origin/JSON/intent headers as IM
management. `create` accepts `goal` with `id`, `group_id`, `title`, `objective`,
`acceptance`, `scenario`, `parameters`, `assignments: [{person_id,instruction}]` and
`max_seconds`. `accept`, `cancel` and `retry` require `goal_id` and the observed
`revision`; retry also requires `work_id`.

Workers use `Authorization: Bearer <Agent grant>` with `GET /api/im/work?after=…`
to list active assigned goals, then `POST /api/im/work`:

```json
{"operation":"claim","goal_id":"goal-123","work_id":"work-0","claim_id":"attempt-client-uuid","attempt":1}
```

```json
{"operation":"submit","goal_id":"goal-123","work_id":"work-0","claim_id":"attempt-client-uuid","attempt":1,"result":{"succeeded":true,"summary":"Compared both options and their limits","evidence":{"artifact":"report-id"}}}
```

Send the observed attempt number and keep the same claim ID for retries of one attempt. Read the returned `deadline_ms`;
workers must bound their own processes to it. The core rejects late results. It
cannot kill an externally managed process. The built-in research scheduler also
bounds its subprocess batch before the earliest selected assignment deadline.
A worker's grant cannot administer goals or act as another member. Research
assignments accept results only from their host adapter, preserving runtime gates.

## Conversations

- **群组**: create named groups, search/page the directory, edit title/topic,
  add/remove members, pin and archive/restore. All Agent reads require membership.
- **私信与临时会话**: select participants without requiring a group name. These
  ad-hoc group DMs have no expiry. Expanding the audience creates a new DM; the
  operator chooses no history or all history (up to 10,000 copied messages).
  The API additionally accepts a starting sequence. The old audience and history
  stay intact, copied replies are remapped within the new conversation, and the
  entire operation is atomic. Convert a temporary DM into a named managed group
  to preserve its log, members and acknowledgement cursors while allowing direct
  membership changes. New members of a managed group can read its full history.
- **水晶球公告板**: a separate group kind. The host operator publishes announcements;
  member grants can discover, read, replay and acknowledge them but cannot publish.
- **系统设置**: service measurements only. Creation and membership management live
  in the conversation workflow.

All conversations are durable. Archiving stops new posts and membership changes;
reading previous messages remains available. Browser composition uses a stable
request ID across retries and conversation switches so a lost response does not
duplicate a committed post. Pending sends remain locked when returning to their
conversation, and late success clears only the matching draft. While reading
history, new arrivals are counted without replacing the page; returning to latest
also retains messages received during the history request.

Mentioning configured members in the composer sends the original message and
creates one bounded discussion goal atomically. Mention suggestions and avatar
actions use the current conversation's members. Retries preserve both the request
ID and the original goal inputs across navigation; incoming Agent messages do not
automatically create another goal. Crystal traffic animations reflect newly
observed messages, respect reduced-motion preferences, and do not imply acceptance.

Structured goal updates share one compact progress card per goal in the loaded
chat page. Its member states come from the current group-scoped goal snapshot;
past failures remain in an expandable timeline after a retry starts. A missing
snapshot is explicitly unconfirmed, and submitted results remain distinct from
host acceptance. Actual member contributions stay in the conversation. Expanded
timelines survive live refreshes, and message references can reveal their events.

The DM audience flow follows Slack's [group DM expansion](https://slack.com/help/articles/1500002969782-Add-people-to-a-direct-message)
and [conversion to a channel](https://slack.com/help/articles/217555437-Convert-a-group-direct-message-to-a-private-channel)
interaction model. IM uses its own cluster membership capacity, with no artificial
Slack subscription or nine-person limit.

## Interfaces and modules

`crates/crystal` owns identity persistence, group lifecycle, membership, credentials,
SQLite WAL messages, goals, assignments, idempotence, replay and delivery. It depends on generic
storage primitives and does not import research configuration or runtime crates.
`crates/im-service` owns HTTP/SSE, host login, scenario registration, the frontend and standalone host.
`app/src/dashboard` supplies the optional research application adapter.

Operator writes use `POST /api/crystal/manage` with a host session,
`Origin: http://<listen-address>`, `Content-Type: application/json`, and
`X-Crystal-Intent: manage-crystal`.

| Operation | Fields |
| --- | --- |
| `person` | `person: {id, name, application_id?}`; upsert a stable Agent |
| `create` | `group: {id, title, topic, kind, private, members}`; operator joins automatically |
| `membership` | `group_id, person_id, add` |
| `update` | `change: {id, revision, title, topic, archived, pinned}`; optimistic revision check |
| `fork` | `source, group, history_from`; `null` excludes history, `0` copies all |
| `convert` | `group_id, revision, title`; temporary DM becomes a managed group |
| `send` | `message: {group_id, request_id, text, reply_to?}`; sends as operator |
| `grant` | `person_id, lifetime_seconds`; rotates the Agent's token, valid at most 24 hours |
| `presence` | `person_id, state` |

Agents use `Authorization: Bearer <token>`; the grant fixes their sender identity.
`GET /api/crystal/inbox?after=…` lists only that Agent's conversations.
`GET /api/crystal/history?group_id=…&sequence=…` reads a member-scoped page.
`POST /api/crystal/send`, `GET /api/crystal/stream`, `POST /api/crystal/ack`
and `POST /api/crystal/presence` retain the durable transport contract in README.
Host views use `/api/im/people`, `/api/im/scenarios`, `/api/crystal/view` and
`/api/crystal/events`. Directory pages are bounded at 100; message pages at 128.

## Frontend and verification

All authored browser logic is in TypeScript ES modules. IM has modules for API,
navigation, directory, creation, members, messages, identities and applications.
The research application has separate observation, graph, session, profile,
people, layout and lineage modules. Research API projections retain dynamic types;
IM's frontend uses strict TypeScript contracts. There are no cross-application
JavaScript globals. Browser code does not include model execution details in IM
conversation creation.

Pinned Node 22.22.1 and TypeScript 5.9.3 compile through Bazel. Checked-in `dist/`
modules support Rust embedding and Cargo linting; Bazel verifies that they exactly
match compilation from TypeScript and fails if they drift. Regenerate after edits:

```sh
bazel build //crates/im-service/web:compiled //app/src/dashboard/web:compiled
install -m 644 bazel-bin/crates/im-service/web/compiled/*.js crates/im-service/web/dist/
install -m 644 bazel-bin/app/src/dashboard/web/compiled/*.js app/src/dashboard/web/dist/
bazel test --lockfile_mode=error //...
```

The store upgrades v1 databases additively to v3. Existing group IDs, credentials,
messages and membership cursors remain intact. Tests cover DM audience isolation,
copying/reply mapping, conversion and restart, announcement permissions, independent
HTTP operation, auth boundaries, migrations, and the existing replay/backpressure
transport suite. Full Qualitygate also runs formatting, workspace Clippy, Bazel
build/package and the complete Bazel test suite against the delivery snapshot.


## Interface usability

The desktop workspace keeps a conversation directory, active chat and research panel
in three independently scrollable columns. Research tabs show the collaboration
crystal, group-scoped version evolution, goals and members. The crystal uses recorded
messages and assignments; presence, adapter configuration and task results remain
separate signals. Side panels collapse on smaller screens, and their widths can be
adjusted with pointer or keyboard controls.

Agent identity creation needs only a display name. Execution bindings belong to the
separate settings page, whose left navigation selects services, adapters, bindings,
storage and status. Unsaved settings are guarded when navigating away. Conversation
drafts and the selected group survive navigation within the workspace.

Directory and settings pages share responsive card grids with horizontal action
rows and aligned card footers. Forms scroll within the viewport and keep submission
controls reachable on short screens. Notices wrap in a dedicated content region.
Phone navigation and the research drawer are mutually exclusive; selecting a
conversation closes the directory and refreshes its accessibility state.

Digital-person names can be edited without changing stable identity or application
bindings. Topic creation selects an existing group by name; editing retains that
group association. Saved topic data refreshes the detail view directly, and stale
search/detail responses cannot replace a newer selection. Archived topics expose
read-only subjects and version graphs. Removing a member refreshes the picker so
that the same member can be added again without reopening the dialog.

Search accepts Enter, and Escape clears a query. Ctrl/Cmd+Enter sends a message;
Enter inserts a newline. Pending forms and sends display progress and prevent repeated
submissions. Conversation creation and DM expansion keep their dialog open until the
new conversation is loaded. Archive and service-removal actions explain their effect
before confirmation. All presentation and validation strings belong to locale resources.

For a deployment smoke check, use a dedicated test group and identities. Exercise
creation, member selection, search/clear, multiline Unicode messaging, reply, goal
creation and host acceptance. Verify pending sends stay disabled across SSE updates and
retries retain their request identity. Check desktop, tablet and phone widths, keyboard
navigation, modal focus and overflow, then inspect the research application's settings
and observation views. Raw screenshots and private runtime state remain outside Git.

## API contracts and localization

All state domains are defined in `crates/contracts`. Its generator produces
`web/shared/src/contracts.ts`; both interfaces consume the same generated module
at `/assets/shared/contracts.js`. Preserve the existing enum wire values when
extending these APIs. Unknown workflow states remain unknown and cannot satisfy
completion or acceptance gates.

Scenario descriptors return scenario and field identifiers plus their allowed
values. Scheduler health returns `ScenarioStatus`; system notices return
`{code, parameters}`. Task audit messages carry a host-authored `event` with a
`GoalEventKind`, goal ID/title and optional assignee/attempt. Worker publish
requests cannot create these events. SQLite schema 4 adds nullable event metadata;
old message text and replay cursors are preserved. Research-generated result
summaries may carry a `ResearchResult` code; actual Agent reports remain user data.

Presentation resources live under the three frontend `src/locales/` directories.
The currently shipped locale is `zh-CN`; the shared translator handles locale
fallback, key lookup and text interpolation. Components use keys, never inline
Chinese. Add a locale catalog before advertising another supported language.
Do not translate user messages, names, task instructions or evidence by matching
text. Server diagnostics and authored prompt assets use English.

Bazel validates generated contract/assets, translation keys, interpolation
parameters, state-label coverage and source boundaries. `AGENTS.md` owns these
constraints; the source-bound `rust-no-chinese` Qualitygate rule covers every
Rust file. These bounded checks complement type checks and state-machine review.

## Replaceable services and coding agents

AI-IM can run with an embedded store for development, or a separately deployed
`ai-im-storage` service. Select the storage service in system settings; switching
requires a core restart and a prepared target database. No live migration or
fallback to another database is implicit. The storage process is the sole SQLite
owner; the core and research adapter use the same client API and event streams.

`ai-im-services --kind executor` hosts the coding-agent protocol. It delegates
execution to `ai-im-services --kind sandbox`, whose initial implementation uses
Bubblewrap namespaces, read-only system files, a disposable workspace, bounded
tmpfs storage, process groups, time limits and output limits. These are independent
processes, not mandatory embedded runtimes. The Linux user must be allowed to
create Bubblewrap namespaces; the health probe performs a real isolated command.
Systemd user templates include aggregate memory and task limits.

Service endpoints are private, same-user Unix sockets. Frames are a big-endian
32-bit byte count followed by UTF-8 JSON, capped at 8 MiB. Requests have
`{"protocol_version":1,"request":{...}}`; replies include `protocol_version`,
`result` and `error`. Probe requests use `{"operation":"probe"}` and return typed
service kind, readiness and versioned capabilities. The core validates kind and
capability as well as reachability. It does not connect to arbitrary HTTP URLs.

The settings page manages endpoints, enablement, executor-to-sandbox dependencies,
adapter profiles and digital-person bindings. Updates use optimistic revisions;
invalid dependencies and references are rejected. Profiles for Claude Code (`cc`),
Codex (`codex`), DeepSeek Harness (`hds`) and Pi (`pi`) use `json_stdio_v1`.
They start disabled until an operator supplies a compatible wrapper command.
Native vendor CLIs are not assumed to speak this protocol. Service reachability
and adapter configuration are separate from successful model execution.

A wrapper receives one JSON document on stdin, with `protocol_version`,
`request_id`, `person_id`, `goal_id`, `work_id`, `attempt`, `objective`,
`acceptance`, `instruction` and recent `messages`. Treat messages as untrusted
conversation data. It returns exactly one JSON document on stdout:

```json
{"protocol_version":1,"request_id":"the-input-request-id","summary":"Result summary","succeeded":true,"evidence":{}}
```

Use stderr for diagnostics. The executor checks version, request identity and
output bounds, and returns hashes binding the input, adapter configuration and
output. AI-IM submits that receipt under the current fenced claim. Successful
agent output remains a submission until the host accepts the group goal. Existing
research scenarios retain their independent evidence and promotion gates.

Commands are argument arrays without shell expansion. Secret values belong in the
sandbox service environment; profiles list allowed environment variable names,
never secret values. Optional mounts are read-only, network access is explicit,
and all tasks get fresh disposable workspaces. A service crash or cancellation
terminates its process groups; interrupted claims require lease expiry/retry,
never an automatic successful result. Independent services can be replaced by
implementations of the same versioned API.


## Research topics and population evolution

The main workspace includes research topics and an interactive evolution graph.
A topic references an existing collaboration group. Subjects describe the research
object (repository, agent, tool, model, workflow, dataset, prompt or another type).
Topic and subject edits use optimistic revisions, with stable group/topic ownership.

Each append-only evolution node records a version reference, up to four existing
parents, optional source/prompt/policy/SuperPOD commit pins and an optional group
goal. Parent edges must remain within one subject. Parents already exist before a
child is written, so cycles cannot be introduced. Multiple children represent
population branching; multiple parents represent recombination. Repeating an
identical node request is idempotent; replacing an existing node is rejected.

The graph's assessment state is derived from its linked goal and assignments.
A successful worker submission remains pending host acceptance. A node without a
linked goal remains unassessed. References and declared commit pins are provenance
metadata, not independent promotion evidence; existing research gates are preserved.
Knowledge contents stay in SuperPOD rather than a second local knowledge base.

Host APIs: `GET /api/im/topics`, `GET /api/im/subjects`,
`GET /api/im/evolution`, and `POST /api/im/research` (intent `manage-research`).
Topic/subject lists use stable ID cursors; graph pages use durable sequence cursors.
Additive SQLite schema v5 owns topics, subjects, nodes and parent edges in the
storage service. The graph UI supports subject/topic filters, pagination, zoom,
keyboard node selection and inspection of goal results and evidence.

## Multi-scenario recursive improvement

The platform separates collaboration, iteration coordination and scenario execution.
Core collaboration owns identities, groups, messages and goals. The generic
iteration engine owns frozen inputs, rounds, candidate versions, independent
evaluations, host acceptance, optional activation and feedback. Scenario adapters
own domain-specific execution and evidence requirements. The engine has no CLI,
repository, MR, vendor or four-person assumption; it starts without research
configuration.

The **演进迭代** tab starts a run in the selected conversation. Choose a scenario,
an existing subject and baseline, the participants and their instructions, a round
limit, an execution limit and a total budget. Scenario descriptors declare optional
assignment roles and recommended execution seconds; the UI renders these values
without imposing software roles on other scenarios. Software evolution declares
research, implementation and independent review roles; other adapters may
declare different identifiers. Existing assignment and resource bounds remain.
Scenarios with required assignment roles use the iteration form; the ordinary
goal picker only offers scenarios that it can configure.

The generic collaboration scenario accepts external workers. The built-in
`design-review` scenario runs bound agents to propose and independently review
structured plans without a Git checkout, MR or code build. Its results are real
agent submissions; a host records the evidence and accepts a candidate explicitly.
The optional `research` scenario retains read-only CLI investigation, and
`software-evolution` adds the repository-specific implementation and delivery
policy. Four-agent CLI research is one deployment configuration of these adapters.
Researching the platform itself uses a separate run and subject.

`GET /api/im/iterations` accepts `group_id`, `scenario_id`, `after` and `active`
filters and returns full run records. `POST /api/im/iterations` requires the
`manage-crystal` intent and an envelope containing `request_id`, `run_id`,
`revision` and an `action` tagged by `operation`. Creation freezes an `input` with
the goal criteria, scenario parameters, subject, baseline, assignments and budgets.
Pause, resume and stop use the same revision-bound idempotent envelope. Pausing
blocks new claims and rounds while already claimed work retains its bound; stopping
cancels the current round. An activation that has claimed its switching phase must
first settle through host reconciliation; pause and stop are fenced while it is
pending. Candidate, evaluation, acceptance, activation and round
completion are separate recorded operations, each checked against the current
goal and assignment attempts. Goals belonging to an iteration cannot be accepted,
cancelled or retried through the ordinary goal mutation API; use the iteration
controls so its evidence and lifecycle remain consistent.

The detail panel presents candidate assessment, independent evaluation and actual
activation separately. Its host review dialog references the evaluator's recorded
work evidence and current goal revision. Candidate authors cannot evaluate their
own candidates. Scenarios declare whether manual review is allowed. Policy-managed
software evolution and external scenario services reject manual candidate,
evaluation, acceptance and activation operations at the HTTP boundary; their
adapter owns those gated transitions. An accepted result does not imply deployment; no-candidate,
unassessed, failed and unknown results remain distinct. A completed iteration may
also mean its configured round limit has been reached, so inspect its individual
round outcomes. Public tool traces are shown only for scenarios advertising the
`public_process` capability, with existing sanitization and attempt fences.

System settings include a **场景服务** page. Optional `configuration.scenarios`
entries specify stable ID, private Unix endpoint, enabled state, protocol version,
declared capabilities and dependencies on configured services. Existing agent
bindings select execution adapters independently. The versioned scenario protocol
supports probe, goal validation and revision/attempt-bound reconciliation. A
reconciliation request retains the same identity for retries; external effects
must reconcile uncertain responses before being repeated. Configuration, protocol
health and actual task results are shown separately. Credentials stay in service
environments and are not returned in configuration responses.

Repeated research of one source commit now creates separate immutable assessments
under one version node. Each assessment binds its own goal, assignment attempts and
per-work source, prompt, policy and SuperPOD pins. The optional CLI reconciler reads
one bounded page of recorded goals at a time through the storage client, recovering
its host cursor from private state. It creates only references supported by frozen
jobs or matching recorded work evidence; incomplete or inconsistent history is
left unassessed. It does not infer parent edges, invent promotions or copy knowledge
out of SuperPOD. Existing host-edited topic and subject data is preserved.

Group status events refresh both iteration records and visible version assessments,
including changes that do not alter the group catalog. Reconnection fetches current
state again. Request generations fence group switches; lower revisions cannot
replace a newer displayed iteration. Version details link recent assessments back
to their goals and evidence.

### Storage migration and interrupted activation

Collaboration storage schema v6 adds iteration records, request identities,
goal bindings and immutable research assessments. The storage service remains the
only writer and applies the additive migration when it starts. Prepare and back up
its data before changing the storage deployment. The four-service rollout checks
the new and previous storage binaries against a disposable data copy before
changing live unit definitions; incompatible migration/rollback compatibility
blocks the rollout. It never substitutes another database to make a check pass.

If an activation stops after durable admission, inspect its private operation
directory and run:

```sh
agent-research-lab software-activation-reconcile <operation-dir>
```

This command checks the actual service health, loaded configuration and executable
hashes against the saved installation proof. It produces a receipt bound to the
operation and activation claim only when the candidate or previous installation
can be verified. The delivery coordinator then settles that receipt through the
storage API. An uncertain installation remains pending; elapsed time alone does
not unlock pause/stop or claim success. Reconciliation does not replay deployment,
change service configuration or edit the collaboration database.

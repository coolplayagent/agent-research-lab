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
request ID across retries so a lost response does not duplicate a committed post.

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

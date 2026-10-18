# 01 Architecture

OutcomeHub is an outcome-based multi-agent work system. See the [current ownership model](../../outcomehub-architecture.md) and [external knowledge configuration](../../external-knowledge.md). Research integrations described below are optional scenarios.

[Documentation](../00-index.md) · [Section index](00-index.md) · [简体中文](../../zh-CN/design/01-architecture.md)

Each library module has its own Cargo package and Bazel package under `crates/`.
The root Cargo manifest is a virtual workspace; shared dependency versions live
in `[workspace.dependencies]` and the workspace lockfile. Library package names
use the `outcome-hub-` prefix, while Rust library names retain the existing module names.

`apps/outcome-hub` is the standalone collaboration application. `apps/cli` owns research command parsing and dispatch; `apps/console` owns the research console and scenario integration. The old `app/` facade is merged into `apps/cli/`; service templates live in `deploy/systemd/`. Libraries do not depend on these applications.

| Crates | Responsibility |
| --- | --- |
| `collaboration-api`, `collaboration-store` | System core: Agent identities, groups, goals, assignments, durable messages, DMs and announcements |
| `collaboration-model`, `service-api` | Database-independent models, validation and service contracts |
| `collaboration-client`, `service-protocol` | Storage client and versioned RPC; no client-owned database |
| `collaboration-storage-service`, `collaboration-store` | Storage service and domain transactions; sole production database ownership |
| `service-registry`, `collaboration-host` | Configuration persistence/probes and application/storage composition |
| `executor-service`, `sandbox-service` | Replaceable execution and bounded sandbox; `service-workers` retains the combined launcher |
| `codex-adapter`, `outcome-service` | Optional vendor bridge, outcome verification and external knowledge updates |
| `software-delivery`, `software-verification` | Research software delivery/activation and shared verification parameters; original gates retained |
| `contracts` | Shared state enums and frontend contract generation |
| `execution-config`, `execution-model` | Configuration declarations and frozen task/attempt records |
| `agent-policy`, `agent-backend`, `isolation` | Permissions, execution adapters and Linux process boundaries |
| `execution-runtime`, `workflow-adapter`, `lease-heartbeat`, `execution-budget` | Scheduling, durable execution, leases and daily accounting |
| `collaboration-proposals`, `agent-teams`, `collaboration-evaluation` | Shared proposals, host membership and bounded collaboration experiments |
| `prompt-evolution`, `prompt-evolution-cli` | Prompt definitions, evaluation and explicit promotion workflows |
| `source-freshness`, `execution-inputs` | Current source/skill verification and frozen research inputs |
| `delivery-adapters`, `knowledge-publication`, `delivery-outbox` | Reviewed installation/publication, external knowledge publication and durable operation queues |
| `execution-targets`, `build-cache` | Disposable CLI targets and isolated build prerequisites |
| `native-effects` | Native filesystem-effect observation, policy and reservation evidence |
| `process`, `filesystem-store` | Bounded subprocesses and host filesystem primitives |

`outcome-hub-cli serve` starts the standalone OutcomeHub system without research
configuration. `--config` enables the optional research scenario in the same service.
The smaller `outcome-hub` binary packages the same core for hosts that only need the
Agent work API. See [Identity, conversations and goals](02-collaboration.md).

Goals and assignments belong to `collaboration-store`, share the durable message writer and
produce transactional group progress messages. Agent credentials can claim and
submit only their own assignments. Successful submissions do not complete a goal;
the host explicitly accepts it. Deadline/claim fencing survives restarts. Active
goals prevent member removal and group archiving that would orphan their work.

`collaboration_api::ScenarioAdapter` contributes parameter choices and admission validation.
The research adapter maps group assignments to immutable runtime Jobs and projects
settled results back to IM. Its bounded scheduler only admits goal-linked IDs;
research gates, persona snapshots, source/prompt/policy/SuperPOD bindings and isolated
memory remain in the research modules. It does not auto-seed unrelated experiments.
The optional research inspector displays these same execution records. Core crates
have no dependency on research configuration or runtime. Frontend behavior lives
in TypeScript modules; the scenario, goal form and goal state views are separate.

## Dependency boundaries

Configuration owns backend declarations and team settings. It validates those
declarations without invoking an execution adapter. Environment-name validation
lives with agent policy, so isolation does not depend on backend execution.

The `execution-model` crate owns `Task`, `Job` and `Launch`. These are data bindings consumed
by orchestration, not a second scheduler. Communication owns the frozen
`LaunchContext`; multi-agent coordination derives membership through
`TeamMembership` using the host's frozen job. This keeps the coordination crate
independent of the runtime implementation.

The runtime owns admission, attempts, durable settlement and recovery. The
collaboration experiment crate may call its explicit host APIs to enqueue frozen
inputs; it cannot redefine execution or communication authority. Application
composition depends on these libraries; none of them depends on applications.

Moving types between crates must preserve JSON field order, serde defaults,
omission rules, enum tags and identity hashing. The compatibility regression uses
serialized output and identity digests produced by the previous implementation.
New crate paths are not a migration of existing private task records.

## Research runtime modules

`execution-runtime` exposes its existing public API through a small crate root.
Private modules own orchestration and supporting operations:

- `scheduler` owns the controller lock, admission order and active workers;
  `launch` owns lease-fenced preparation and spawn, while `recovery` reconciles
  retained attempts. All three use the same completion and admission guards.
  Inside `scheduler`, `cohort` retains prerequisite backoff, `workers` owns
  process/heartbeat guards and settlement, and `eligibility` shares the workflow
  and cohort admission rules. The controller retains the original budget window
  and latches guarded failures across polling rounds.
- `job_store` owns durable research job reads and replayable workflow starts;
  `jobs` handles enqueue identity and bounded retries. Ordinary and batch
  registration share `registration` and `job_store`.
- `host_budget` bounds host work and worker allowance; `scope` validates the
  selected task set, `seeding` creates optional research cohorts, and `status`
  projects controller states without admitting work. `worker_identity` and
  `git` provide shared process-identity and source observations.
- `prompts` renders frozen context; `followups` owns proposal authority, output
  schema and deterministic child registration.
- `receipts` validates committed results and dependency bindings; `completion`
  settles observed attempts with their existing leases. `candidate` owns host Git
  snapshots and protected-path checks.
- `memory` handles bounded task-local checkpoints and CLI calls; `post_success`
  journals and recovers memory and follow-up effects. `team_channel` handles
  retained communication membership and diagnostics.

The modules preserve existing runtime function paths, serialized records and
research gates. Batch registration and worker execution share prompt rendering;
persona and task memory share checkpoint formatting. Production modules declare
their imports explicitly. Unit tests live in sibling
`*_tests.rs` files. The [local semantic review](../../../qualitygate/reviews/runtime-boundaries.md)
records ownership, unchanged authority checks and verification limits.
The [scheduler phase review](../../../qualitygate/reviews/scheduler-phases.md)
records the later separation of controller state and its behavioral regressions.

## Documentation and build assets

Each crate declares dependencies in Cargo and Bazel. Prompts, examples and desktop assets are explicit compile inputs. The main package includes both documentation languages. See the [development guide](../guide/08-development.md) for commands.

The durable production database belongs to the storage service. `collaboration-store` implements its domain behavior; API consumers do not open the live database. `contracts` owns shared state enums, and `service-protocol` defines replaceable service messages.

## Outcome collaboration

Outcome collaboration connects goal paths within topics through immutable publications, explicit references, independent verification and host acceptance. SuperPOD is an optional knowledge binding; replaceable services update external repositories. See [architecture](07-outcomes.md) and [usage](../guide/09-outcomes.md).

## Independently buildable boundaries

- Models and clients do not depend on `collaboration-store`, `collaboration-storage-service` or SQLite. The HTTP library also excludes storage implementations. `collaboration-store` re-exports the previous model names for Rust callers.
- `collaboration-host` selects the configured production storage endpoint or explicitly creates an independent embedded development adapter. Both use the same `collaboration-client` request contract.
- Messages, goals, outcomes and coordination lineage retain their shared transaction boundary for membership, attempt fencing and acceptance consistency. Research execution, prompt evolution and SuperPOD research publication remain optional extensions outside the core package.
- `//:outcome-hub-package` contains core collaboration and storage; `//:adapter-package` contains optional services; `//:package` remains the complete research distribution.
- Bazel analysis tests in `verification` traverse actual transitive dependencies and package contents. Pre-refactor protocol samples, client transport parity, standalone startup/restart and repository regression tests verify behavior. Semantic review is recorded in `qualitygate/reviews/module-boundaries.md`.

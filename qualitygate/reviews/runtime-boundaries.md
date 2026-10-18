# Research runtime boundary review

Baseline: `acb23588ceebe93e3f21248c7f20fcd0e8d2b370`.
This is a local implementation and semantic review, not independent signed approval.
Acceptance requires the full snapshot-bound Qualitygate and repository tests.
The initial completion extraction is recorded first; the orchestration follow-up
below records the current ownership relative to its own baseline.

## Ownership

The runtime remains an optional research adapter. This change creates private
modules inside the existing crate; it adds no service, dependency, public module,
database owner, lifecycle enum or wire format. Existing public function paths are
preserved through re-exports. The scheduler still owns worker admission and the
controller lock.

| Module | Responsibility and boundary |
| --- | --- |
| `prompts` | Render frozen role/task/input/persona prompts; registration and execution use the same functions. |
| `followups` | Define proposal authority and schema, validate proposals and register deterministic child IDs through the existing enqueue entrypoint. |
| `candidate` | Capture host-owned Git snapshots after checking protected paths, baseline, detached state and Git authority. |
| `receipts` | Validate report structure, workflow success, dependency agreement and frozen experiment bindings. Raw worker files cannot authorize a completed result. |
| `completion` | Settle observed worker results using the retained attempt and execution lease; it does not enqueue tasks or perform memory writes. |
| `memory` | Produce bounded checkpoints and invoke an explicitly configured memory executable with separate per-task state. |
| `post_success` | Persist and recover the completion journal before memory writes or follow-up admission. |
| `team_channel` | Revoke retained membership intent and record bounded communication diagnostics. |

New production modules use explicit imports. Batch prompt binding imports
`prompts` directly, persona writeback imports `memory`, registration reads
`receipts`, and admission failure settlement imports `completion`. The root
continues to compose these operations; it does not re-export their private
implementation helpers as new public APIs.

## Semantic review

- Completion retains the existing observation-success, installed-byte, report,
  private-tool-snapshot and candidate checks before publishing a result. Rejected
  read results fail; uncertain writes remain unknown. Lease heartbeats and saved
  lost-reply results retain their original ordering and deadlines.
- Receipt projection requires a known workflow success and exact source, prompt,
  policy, model, research-input and backend bindings. Dependency loading additionally
  checks local/committed agreement and per-report/aggregate size bounds. The three
  receipt readers retain their distinct existing contracts; no gate was generalized
  away during extraction.
- Post-success processing persists `Running` before an external write and never
  repeats an unknown memory outcome. Retryable follow-up admission retains stable
  proposal indices, fixed-scope deferral and exact-task enqueue idempotency.
- Follow-up authority remains bounded by depth, role, write permission, tool
  allowlists and proposal count. Prompt instructions, output schema and admission
  validation still share this policy owner. No worker proposal is host acceptance.
- The memory call still uses a task-specific home and explicit local backend;
  full provenance stays in the workflow receipt. Prompt text and serialized
  authority data retain their original bytes and fields.

## Evidence and limits

The extraction audit compares all 73 original non-test root function bodies,
preserving expression and literal tokens while ignoring comments and whitespace.
This supports the mechanical-move claim, not runtime correctness by itself.

Seventeen existing tests now live beside the modules they exercise. Existing
recovery tests cover forged local evidence, interrupted memory, deterministic
follow-up replay, scope deferral and invalid proposal siblings. New receipt tests
exercise changed frozen bindings, unknown/non-success states, forged local reports
and stale dependency pins through the same workflow-client fixture.

Rust compilation checks visibility and call sites. The code graph is navigation
evidence with inferred edges, not a semantic proof. Full Bazel tests additionally
cover wire compatibility, package dependencies, standalone collaboration and
frontend contracts/locales. Existing ignored external CLI/model tests remain
ignored; deterministic fixtures do not certify live external services.

## Orchestration follow-up

Baseline: `7bf230f7a294c5c06822662e8408a62446797688`.
This follow-up is also a local implementation and semantic review. It preserves
the service, protocol and evidence boundaries above. The crate root now only
declares modules and re-exports the existing public API.

| Module | Responsibility and boundary |
| --- | --- |
| `scheduler` | Own the controller lock, observer lifecycle, worker set and existing admission/cleanup order. |
| `launch` | Verify frozen inputs, acquire and claim the current attempt, persist launch authority, then spawn within the original deadline. |
| `recovery` | Cancel matching retained process groups and reconcile abandoned attempts using their original authority. |
| `job_store` | Read research jobs, construct the workflow adapter and replay the frozen start record. This is not collaboration database ownership. |
| `jobs` | Validate ordinary enqueue identity and prepare bounded read-only retries; call the shared registration owner. |
| `host_budget` | Persist and charge host intervals, honor pause/daily/run limits and bound prerequisite work. |
| `scope` | Validate a private task-ID file, configuration pins and dependency closure; annotate the existing status projection. |
| `seeding` | Create optional research cohorts from frozen inputs and persona selections. |
| `status` | Observe workflow state, distinguish blockers from live execution and expose separately labelled capability probes. |
| `worker_identity`, `git` | Share process start identity and bounded Git observations without owning task state. |

Production modules no longer import a parent wildcard. In particular, batch
validation imports the frozen input/start owner directly, and persona memory
imports only its directory operations and task checkpoint formatter. Registration
and retries both use `job_store` for replay; registration does not depend on the
enqueue module. The scheduler composes these owners without adding new state
enums or a second state machine.

The semantic review checked these preserved orderings and conditions:

- Controller cleanup still cancels matching retained workers before budget or
  pause can prevent CLI recovery. Process identity still includes Linux start
  time; recovery cannot use a replacement lease to authorize an old attempt.
- Source freshness is rechecked after all prerequisite probes and before claims.
  Launch still verifies frozen prompt/config/backend/dependency evidence, records
  authority before spawn and consumes final hook time from the same deadline.
  Guarded job writes still fence both task attempt and retained launch identity.
- Host work persists its active budget interval before effects and charges failed
  work. Expired prerequisites defer without claiming or permanently rejecting a
  task. No budget is restarted by moving the implementation.
- Status parses shared workflow enums at the boundary; unknown state cannot
  authorize success or admission. Summary projection still drops raw workflow
  receipts between observations. Scope validation and retry ceilings are unchanged.
- Seed and follow-up processing remain subject to explicit scope restrictions.
  Core collaboration has no new dependency on research configuration or SuperPOD.

The extraction audit compares 52 original Rust items, including struct fields
and impl blocks. Expression and literal tokens are identical after allowing only
private visibility changes and the qualified test-fixture path. All 94 runtime
test bodies and attributes are preserved, including the 35 existing ignored
external integration tests; twelve tests move beside their new owners. Compilation
and the runtime tests check cross-module visibility and existing behavior. Final
acceptance additionally requires full Qualitygate and all Bazel repository tests.
These checks do not establish live external provider compatibility or deployment
readiness.

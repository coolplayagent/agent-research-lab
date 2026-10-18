# Software activation boundary review

Baseline: `94c3f538367f4b2c4b43d7f70ab324fc76a2e33f`.
This is a local implementation and semantic review, not independent signed
approval. Delivery requires repository tests and full snapshot-bound Qualitygate.

## Ownership

The optional research adapter keeps the same three public functions: `request`,
`worker` and `reconcile`. Private modules add no external service, configuration,
database handle, protocol revision or lifecycle enum. Frozen record definitions
retain their field order, serde defaults, omission rules and strict readers.
Activation states remain distinct shared enums in `contracts`.

| Module | Responsibility |
| --- | --- |
| `request` | Validate frozen inputs and policy; inspect retries or start the bounded worker. |
| `worker` | Hold the worker lock and deadline; check frozen files, build, dispatch the selected rollout and settle its outcome. |
| `reconcile` | Confirm durable ownership and prove candidate or baseline installation without replaying deployment effects. |
| `model` | Define the existing policy, request, prepared operation and installation proof records. |
| `build` | Check upstream and candidate/merged tree identity, run configured builds and require the exact merged full gate. |
| `cli` | Prepare and bind an immutable skill package, switch configuration and restore it on failed health. |
| `platform` | Prepare service artifacts, stop/start in dependency order, check offline storage compatibility and restore units on failure. |
| `admission` | Confirm iteration claims through the storage API; drain native jobs and release only the owning operation's marker. |
| `journal` | Persist local admission intent, switch markers, phase observations and bound receipts. |
| `service` | Parse unit identity, observe running artifacts and probe actual HTTP health. |
| `host` | Preserve bounded commands, package copies, path checks and synchronized file replacement. |

Imports are explicit and follow the operation stages toward shared helpers and
records. Neither `model` nor the host primitives depend on orchestration. CLI and
platform rollout do not call each other. Request and reconciliation do not call
the worker or replay its effects.

The stale direct `execution-runtime` dependency is removed from Cargo and Bazel.
Native job inspection already uses `execution-model`; durable iteration changes
already use `collaboration-client`. This removes the research scheduler from the
activation library's dependency closure. The new Bazel analysis test traverses
the actual transitive graph, rejects runtime and database implementation edges,
and requires the task model, storage client and shared verification parameters.

## Semantic review

- Request validation still checks release evidence before creating an operation.
  The required research source/prompt/policy/SuperPOD and independent evaluation
  gates remain in the unchanged evidence validator. Policy and configuration
  hashes, request identity and existing receipt checks retain their order.
- Build still rechecks the upstream default, fetches exact commits and builds a
  detached merged worktree. The candidate and merged trees must match. Clean
  source checks surround the exact merged-commit full gate, followed by a remote
  binding recheck. Missing/unknown gate fields cannot imply a pass.
- Admission still checks the running iteration, accepted unfinished round and
  exact candidate version. It journals the revision-bound command before RPC.
  Local intent alone cannot establish a claim: recovery consults the durable
  round, and uncertainty keeps the drain in place.
- The worker lock and deadline stay in `worker`; the drain guard stays in each
  rollout through the terminal receipt construction. Moving the CLI tail into
  `activate_cli` preserves guard lifetime relative to all side effects and
  returns before worker settlement, just as the original `activate` did.
- Both rollouts persist the switch proof, acquire the durable activation claim,
  mark the switch and record its phase before changing live configuration or
  stopping services. All process/resource bounds and command arguments remain.
- Platform activation stops main, sandbox, executor and storage in reverse
  dependency order before copying offline data. New and previous storage
  binaries each open the disposable compatibility copy, never a shared live
  database handle. Startup waits for storage readiness before dependent units.
  Failure restores previous unit files and restarts them without replacing live
  storage data. CLI failure restores the previous configuration and checks health.
- Worker failure records `Blocked` unless rollback was confirmed; it removes a
  drain only after rollback or confirmed absence of admission. Reconciliation
  checks actual configuration, unit and running-artifact hashes plus health;
  it cannot accept a local claim without the matching pending durable round.
- Receipt fields and hashes are unchanged. A successful worker or reconciliation
  persists its receipt before terminal phase and drain release. These operations
  do not independently promote a candidate or write external knowledge.

## Evidence and limits

The extraction audit compares all 47 original records, implementations and
functions, including public signatures. It expands the new CLI delegation before
comparing operation order; only private visibility, formatting commas and
equivalent immutable borrows are normalized. All six existing test bodies and
attributes are preserved, including the legacy policy bytes/digest regression.
This structural check supports, but does not replace, the semantic review above.

Seven new regressions exercise missing/malformed admission journals, matching
pending claim requirements, receipt evidence binding, drain ownership, malformed
full-gate reports and the public worker/reconciliation early-failure paths. The
focused Bazel target passes all 13 tests. Recovery fixtures use private temporary
files and an absent storage endpoint; they do not invoke production activation.

Full Bazel checks additionally cover API consumers, durable iteration fencing,
shared contracts, locale boundaries, core startup and packages. These fixtures
do not establish live systemd rollout, real storage migration compatibility,
external CI outcomes or production rollback success. Those still require actual
deployment evidence; this refactor does not claim it.

# Multi-scenario iteration ownership and presentation review

This is an agent-authored implementation review, not independent approval or a
research promotion. The final repository tests and full Qualitygate report must
bind the delivered snapshot. This note records inspected ownership and focused
regression evidence; it does not substitute for those gates or live deployment
checks.

- Collaboration storage owns the durable iteration and version-assessment records.
  Its serialized commands perform revision, request identity, goal-attempt and
  independent-evaluator checks in the same writer. The web service, research
  reconciler and software adapter use the storage client. The optional CLI
  reconciler reads its own bounded frozen job files, never the collaboration DB.
- Scenario descriptors and registration contain stable identifiers, optional roles,
  declared execution limits, capabilities and protocol health. Frontend role
  controls consume the descriptor's arbitrary role identifiers. Native process
  disclosure is descriptor driven. No four-agent limit, repository list or MR
  operation was added to the generic UI or iteration records.
- The UI obtains candidate/evaluation/activation state labels from generated Rust
  contracts and locale catalogs. Missing evaluation is unassessed; host acceptance
  without activation remains visibly unactivated. User instructions, evidence and
  identifiers are inserted as text, without translating or interpreting user HTML.
- The start form scopes subjects and baselines through topics owned by the selected
  group and freezes the selected group for submission. Control requests carry the
  displayed revision and retain their request ID on response-loss retry. A changed
  payload has a different identity. Group generations reject retired list replies;
  a lower run revision cannot replace a newer local snapshot.
- Host review reads the current goal before collecting evaluator evidence and
  excludes the producer's person identity from evaluator choices. It sends the
  recorded evidence and goal revision, so backend concurrency/attempt checks remain
  authoritative. Host review is a policy boundary: the HTTP service rejects manual
  promotion operations for scenarios declaring `manual_review: false`, including
  policy-managed software delivery and external services. Their adapter retains
  the tests, Qualitygate, independent review, CI and activation gates. The UI hides
  manual review for these descriptors; that visibility is not the enforcing gate.
  Direct accept/cancel/retry on an iteration-bound goal is also rejected by storage;
  goal cards use the server's additive `iteration_id` projection to link to the
  owning iteration instead of exposing conflicting goal controls.
- Group status and reconnect handlers refresh visible version and iteration data
  even when the conversation catalog revision is unchanged. Repeated CLI studies
  can appear as distinct assessments of one commit with different per-work prompt
  pins. The reconciler refuses mismatched native identities and mixed-source work
  sets; it never guesses graph parents or claims acceptance from execution success.
- Network deployment uses a concrete interface address and management credentials.
  Loopback-only local-operator mode is not a remote authentication shortcut. Host,
  Origin, mutation intent and scoped bearer checks remain separate boundaries.

Focused Bazel frontend evidence: `iteration_tests` exercises arbitrary scenarios
and six participants, absent evaluation, acceptance without activation, preserved
user text, old-group replies, revision monotonicity and retry request identities.
`workspace_tests` adds goal-only update and reconnect counterexamples for the
version graph. Existing layout/bootstrap tests and `//web:locale_tests` pass on
the reviewed frontend snapshot. Source-level and live browser review must also
verify the integrated final backend snapshot; these tests do not prove MR delivery
or deployment success.

## Software delivery and activation semantic review

The UI agent separately traced `software_scenario`, `software_delivery`,
`software_activation`, the native candidate capture, the iteration writer and
the HTTP/remote scenario adapters. The relevant invariants are:

- A software producer cannot self-review. The configured role pipeline has one
  implementer and one other reviewer; the reviewer consumes the captured candidate
  commit. The host checks the exact candidate worktree, repository tests, full
  Qualitygate, immutable evidence bindings and an explicit independent verdict.
  A successful worker exit or a declared adapter capability is insufficient.
- Candidate acceptance records a passed assessment. It occurs before PR checks
  and deployment, so it does not mean that a PR merged or a service runs that
  version. Merge checks re-read the expected PR head and current upstream. The
  merged tree must match the verified candidate; activation performs a second
  full check bound to the actual merged commit and checks real service health.
- The iteration baseline changes only when an accepted round completes. A failed
  or rolled-back activation cannot complete that round. Graph assessment labels
  describe host acceptance, while the iteration detail separately presents actual
  activation. Missing evidence and pending activation cannot display as active.
- HTTP manual review is denied for policy-managed scenarios. Remote scenario
  responses are checked for request, run revision, goal revision, work attempts
  and registered capabilities before the storage writer applies the action. A
  registered remote scenario is a trusted host adapter; this protocol does not
  prove the truth of an external service's scientific claims.

The review found concrete counterexamples requiring closure before delivery:

| Finding | Required correction and verification |
| --- | --- |
| The native candidate protected `.qualitygate/` but allowed the actual `qualitygate/rules/` policy sources to change. | Protect the entire `qualitygate/` tree and retain a candidate-capture regression for that path. |
| A detached activation worker could continue switching after the iteration was stopped. | Claim activation durably through the storage writer before switching; atomically fence pause/stop while that claim is pending. Verify recovery and stale-worker rejection. |
| A blocked or interrupted activation was described as retaining the previous installation, even when its actual state was uncertain. | Distinguish confirmed rollback from blocked/unknown effects. Do not report a retained installation without evidence. |
| The scheduler restarted pagination at the first four active runs on every tick. | Rotate through active runs while bounding concurrent reconciliations; test five or more runs with the first four waiting. |
| The ordinary goal form offered role-based scenarios without controls for their required roles. | Keep those scenarios in the iteration form, which renders descriptor-declared roles; the ordinary goal picker excludes them. |
| `crypto.randomUUID()` is unavailable at a plain HTTP interface address, blocking messages and creation dialogs. | All callers import the shared cryptographic UUID helper, using `crypto.getRandomValues()` without patching globals. The HTTP-context regression covers actual WebCrypto, dialog creation and stable message retry identity. |

The first four backend findings were sent to their owning agents. Source review
and the owners' focused test results now close the reported counterexamples:

- Runtime capture protects `qualitygate/`, and its focused Bazel tests pass.
- Atomic `BeginActivation` records `Pending` in storage. The core regression
  `activation_admission_fences_pause_stop_and_requires_its_terminal_receipt`
  covers pause winning a race, stale admission, pending pause/stop/finish refusal,
  and a matching terminal receipt after reopening storage.
- The follow-up review found that a local admission journal alone could leave a
  phantom claim after rejected RPC admission. The worker now confirms the durable
  record; `rejected_admission_does_not_leave_a_phantom_pending_activation` covers
  safe drain release before switching, while unknown outcomes remain fenced.
- Confirmed rollback is recorded as `RolledBack`; blocked activation makes no
  claim that the previous installation was retained. Probe-only recovery checks
  real installation proofs without repeating deployment effects.
- The scheduler retains its cursor across concurrency-limited ticks. Its real
  five-run regression starts the fifth goal while the first four wait.

The core owner reports focused app, crystal, im-storage and runtime Bazel targets
passing; the service owner reports all six iteration regressions passing.
These are focused results, not a claim that the final full gate has passed.
Frontend regressions
cover the ordinary-goal routing and the pending-activation controls. The pending
view disables pause, stop and manual review and explicitly asks the user to wait
for host settlement; the backend remains responsible for enforcing that boundary.
The final combined snapshot still requires the coordinating agent's gate. The
focused Bazel `//web:identity_tests` passed with `randomUUID` absent and a non-secure
context; a real interface-address browser check remains required.

After the pending-activation and HTTP identity corrections, the six focused Bazel
frontend targets (`identity_tests`, `locale_tests`, `iteration_tests`,
`workspace_tests`, `layout_tests` and `bootstrap_tests`) all passed. Shared, IM and
research-dashboard TypeScript generation and checked-asset targets also passed,
including the generated `Pending` and `ApiErrorCode` contracts. No production
message or iteration was created by these tests.

Remaining operational verification is explicit: the live browser must show the
four agents' real group messages and process events, current iteration status and
updated graph after deployment/reconnect. The review did not run a destructive
real production rollback or kill a production activation midway through switching.
Crash recovery, database migration compatibility and external CI/merge outcomes
must be supported by their focused tests and durable receipts, not inferred from
frontend tests. The final repository-wide tests and full Qualitygate are owned by
the coordinating agent and were intentionally not started concurrently here.

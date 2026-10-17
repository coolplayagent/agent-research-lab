# Outcome collaboration domain review

Codex performed this local implementation review on 2026-10-17. It is an
agent-authored review of domain ownership and state transitions, not independent
approval or signed acceptance. Delivery still requires the final full gate.

## Ownership and compatibility

- `crystal` implements outcome transactions in the existing durable owner;
  `im-storage::Client` exposes the same command protocol for local and remote
  adapters. No new service opens the production collaboration database.
- Schema 7 adds paths, outcomes, references and append-only events. Existing
  message, goal, iteration and research lineage tables and serialized states
  retain their values. Migration and old-flow tests remain in the full suite.
- Outcome lifecycle enums have one definition in `contracts`, and TypeScript
  contracts are generated through Bazel. Feature modules use these enums;
  locale catalogs carry presentation labels and translated errors.
- SuperPOD is optional for core outcome collaboration. Source/prompt/policy pins
  remain required, and the existing research adapter's SuperPOD and promotion
  gates are unchanged. The generic knowledge adapter writes external import
  branches; it does not duplicate a knowledge database or bypass SuperPOD maps.

## Authority and transitions

- Agent publications are authenticated again inside the write transaction and
  require live topic membership and the current running claim/attempt lease.
  Attempts cannot manufacture verification, acceptance or knowledge receipts.
- References point to existing same-topic outcomes; author and input digest,
  immutable IDs and atomic insertion prevent rewriting provenance or cycles.
  Withdrawal and current state remain visible to consumers. Reference counts
  are discovery heuristics with capped ranking contribution, never promotion.
- Verification requests bind outcome/configuration/request digests and a
  deadline. Passed/failed receipts require evidence. Unknown external enum
  values fail decoding and become unknown results; cancellation, replacement,
  receipt mismatch and service binding changes cannot grant acceptance.
- Acceptance names the current passing verification; a new verification
  invalidates the previous current projection. Goal completion and research
  assessments retain their own state machines and gates.
- Knowledge dispatch requires accepted current evidence, a configured target
  and expected revision. It rechecks acceptance after service preflight.
  Since external effects cannot be atomically rolled back with collaboration
  state, immutable receipts retain an already-applied write even if acceptance
  later changes. Unknown effects block a new write until reconciliation.
- Dispatch is persisted before a bounded worker starts. Automatic recovery does
  not replay writes. The Git service keeps private idempotency records and a
  receipt in the target commit, uses an isolated index and compare-and-swap,
  and rejects checked-out target branches. Lookup has no remote write effects.

## Evidence and scope

Storage tests exercise role boundaries, immutable/idempotent publication,
attempt fencing, cross-path and cross-topic references, membership removal,
restart durability, independent acceptance, unknown write reconciliation and
bounded snapshot pagination. HTTP tests exercise bearer discovery, forbidden
internal receipt commands, absent optional configuration, automatic verifier
protocol replies and unknown-state rejection. Git adapter tests perform real
local Git publication, expected revision conflicts, duplicate requests, lost
local receipt recovery, checked-out branch rejection and path injection checks.

Frontend compilation, generated-asset drift checks, locale key/parameter tests
and module-serving tests are mandatory. Browser walkthrough uses a disposable
local service, fixture outcomes and no external production repository. Full
Bazel and Qualitygate reports provide final snapshot evidence separately.

No model efficiency or scientific validity claim follows from these tests.
Semantic verifier quality depends on the explicitly configured checks and
host review. Same-user private sockets are trusted host interfaces, not an
untrusted execution boundary. The first release deliberately has manual paths,
same-topic sharing and explicit knowledge publication; it has no dynamic
scheduler, public cross-topic search or automatic default-branch integration.

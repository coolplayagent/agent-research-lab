# Coordinator protocol recovery review

This is an author review of ownership and state-machine behavior, not independent
approval. The reported paused intake retained only `task_protocol_invalid`, so
its original rejected response cannot be reconstructed. A private replay with
the same intent, available members and conversation context returned a valid
plan. The fix addresses the observed immediate-stop behavior and missing
failure evidence, without asserting which original response field was invalid.

- The collaboration storage writer still validates and commits each proposal in
  one transaction. Rejected proposals cannot partially publish a graph, chat
  events, artifacts or acceptance. The storage regression exercises rejection,
  restart, correction, and rejection of an obsolete claim.
- Protocol rejection now uses the existing consecutive-attempt limit, active
  time budget and overall coordinator call cap. No failed result becomes a
  successful result. Invalid schema, unknown protocol versions, unsuccessful
  execution, invalid graphs and premature delivery continue to fail validation.
- Each retry obtains a fresh lease and input digest. The next prompt identifies
  the previous rejected attempt using its typed error, without treating private
  diagnostic text or rejected model content as instructions. A successful turn
  ends the consecutive failure streak.
- Coordinator execution explicitly requests a coordination proposal. The user
  intent remains intact in the task context; future worker completion and human
  acceptance are separate from success of this proposal-producing turn.
- Private registry diagnostics retain the validated execution receipt on parse
  and storage-commit rejection, bounded to 64 KiB, plus a bounded error chain.
  They remain outside the public task API and repository. Public error codes
  and existing serialized states are unchanged; presentation stays in locales.
- A failed service/configuration probe is no longer always labeled a protocol
  error. Protocol-check errors and unavailable execution remain distinct.

Coverage: coordinator parser reason tests; bounded consecutive retry tests;
transactional storage recovery and stale-claim tests; prompt recovery context;
private diagnostic binding and size limit; full repository Bazel suite and
Qualitygate required on the final snapshot. Model output remains probabilistic;
exhausted retries pause the task for explicit operator action.

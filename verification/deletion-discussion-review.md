# Deletion and independent discussion review

This is an implementation source review, not independent approval or host acceptance.

## Durable ownership and references

HTTP deletion requires the existing loopback host intent boundary and rejects agent grants. The request crosses the versioned collaboration client to the storage owner. Revision checks, reference checks and deletion occur in the same writer transaction. Active goals and evolution runs prevent deletion. Recorded current or historical work results, version nodes, assessments, outcome paths, outcomes and evolution records prevent group deletion. Topic deletion cannot cascade into version nodes or outcome records. Cancellation followed by a late submit cannot recreate a removed goal: existing goal and claim checks still apply.

Announcement bodies are erased while sequence, request identity and reply references remain. Empty message text is reserved for this placeholder because ordinary publishing rejects empty content. The optional `deleted` field defaults to false for old readers of persisted messages and is omitted on ordinary wire messages. No schema migration or external knowledge mutation is required. A duplicate publishing request cannot restore a deleted body. Existing backups are outside this operation.

The writer reloads its catalog after deletion and closes the old broadcast feed. Closed subscriptions reject buffered pre-deletion content; an old subscriber cannot remove a replacement feed on drop. Directory and conversation events update browser views. Topic subject listings expose existence separately from an empty subject list. A revision fence prevents an earlier board refresh from replacing newer content.

## Execution semantics

Only the generic `collaboration` input gets individual contribution criteria. Other scenarios keep their original acceptance input, including research evidence gates. The original goal criteria remain in the generic input's host context and remain authoritative in persisted goals. Parallel scheduling is unchanged. Adapter receipts are still bound to adapter, input, output and current claim; receipt success produces a submission, never host acceptance. Failure answers are visible with their failed state. Matching a result requires the same group, goal, work, person and attempt.

## Executable evidence

Storage regressions exercise durable deletion, stale revisions, protected lineage, cancelled retries with historical reports, announcement reply preservation, closed feed buffering, and duplicate-request rejection. API tests exercise host intent versus agent grants and additive wire compatibility. Remote storage tests exercise the same client operation over the service protocol. Frontend tests cover confirmation cancellation, reference errors, deleted replies, full failed answers and stale-attempt rejection. The complete Bazel suite and full Qualitygate report are required for the delivery snapshot; runtime preview additionally exercises browser synchronization and parallel execution.

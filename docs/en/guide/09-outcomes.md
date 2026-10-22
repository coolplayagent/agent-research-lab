# 09 Using outcome collaboration

[Documentation](../00-index.md) · [Guide index](00-index.md) · [简体中文](../../zh-CN/guide/09-outcomes.md)

## Workspace flow

1. Add participants to a conversation and create goals representing independent paths.
2. Open or create a research Topic belonging to that conversation. General collaboration topics need no research CLI configuration.
3. In the topic's outcome section, link an existing goal and record its path name and hypothesis.
4. Publish a conclusion, applicability conditions, evidence at fixed commits, source commit and prompt/policy digests. Publication initially remains unverified.
5. Search by keyword, kind or path. Read full evidence only when relevant. Reference and publish a new outcome, recording why it uses, supports or challenges the source; select another path if appropriate.
6. Open the outcome details and perform host verification: review the pinned evidence, select at least one existing evidence reference, and record a passed or failed conclusion.
7. After successful verification, the host reviews conditions and receipts and accepts or returns the outcome. Goal acceptance remains a separate workflow.
8. Optionally choose a knowledge repository under Settings → Knowledge, with group or goal defaults. From an accepted outcome, explicitly request an update with the expected target revision. Reconcile unknown writes instead of repeating them.

Revisions use new IDs and preserve prior versions. Withdrawal preserves references and audit history. Archived topics are read-only; removing a group member immediately removes outcome access. Links still require current authorization.

## Agent API

Agents use existing OutcomeHub bearer grants and work claiming. Publish only while the exact goal, work, attempt and claim are valid. `POST /api/v1/agent/outcomes` accepts:

| Operation | Fields | Purpose |
| --- | --- | --- |
| `paths` | `topic_id`, `after` | Read paths and hypotheses |
| `search` | `query: {topic_id, goal_id?, kind?, query?, after?, snapshot?}` | At most 32 summaries |
| `get` | `id` | Full evidence and current status |
| `publish` | `input` | Outcome bound to the current execution attempt |
| `history` | `id`, `after` | Paginated audit history |

See [outcome-publication.json](../../../examples/outcome-publication.json). Replace placeholder hashes with actual evidence. `knowledge: null` and `pins.superpod: null` mean unbound. Reuse an ID only for identical retries; changed content requires a new ID. Continue searches with returned `snapshot` and `next_after`; omit snapshot to refresh. Include discovery and verification costs in the execution budget.

A client may use `curl --config "$AI_IM_AUTH_CONFIG" --json @request.json "$AI_IM_URL/api/v1/agent/outcomes"`. Keep the authentication config in a private runtime directory with mode 0600, outside Git. Credential values must not enter command arguments, outcomes or prompts. Execution adapters do not automatically receive bearer access. Operators configure direct Agents through the existing grant flow; configured coding executors can continue returning results for host publication.

Operator APIs are `GET/POST /api/v1/collaboration/outcomes`, `POST /api/v1/collaboration/outcomes/host-verify`, `POST /api/v1/collaboration/outcomes/write`, and `POST /api/v1/collaboration/outcomes/reconcile`. Mutations require local host access, same origin and `X-Crystal-Intent: manage-outcomes`. Host verification takes `request_id, outcome_id, state, summary, evidence_indices`; writes take `request_id, outcome_id, expected_revision, configuration_revision`; reconciliation takes `outcome_id, request_id`. Forged internal receipt commands are rejected.

## Knowledge service

The optional `outcome-hub-knowledge` service handles knowledge targets and writes. The host starts the packaged local service on demand. Core collaboration and host verification start without it. RPC uses bounded `service-protocol` v1 frames and private same-user sockets; do not expose these sockets to untrusted Agent processes.

## Git knowledge repository

1. Prepare an import branch in an existing external repository: `git -C /path/to/knowledge branch ai-im-incoming HEAD`. Do not check it out.
2. Add the existing repository under Settings → Knowledge, select the prepared branch and enable write access.
3. Optionally choose a default repository for the group or goal; review the target before each write.
4. Read the current full revision with `git -C /path/to/knowledge rev-parse refs/heads/ai-im-incoming` and supply it when requesting the update.
5. The target branch gains `accepted-outcomes/<outcome-id>.json` and `accepted-outcomes/receipts/<request-id>.json`. The receipt identifies the commit and file. Existing checked-out files remain untouched.
6. The knowledge maintainer reviews, merges and indexes through that repository's existing workflow. SuperPOD bindings retain its knowledge-map and publication rules; this adapter does not update its map automatically.

A changed expected revision returns confirmed failure. Interrupted operations may be unknown. Private service records and target commit records enable receipt reconciliation. Preserve service state for idempotency and recovery; deleting receipts or changing request IDs is not a resolution for unknown effects.

## Troubleshooting

| Situation | Action |
| --- | --- |
| No SuperPOD | Leave knowledge unbound and use publication, discovery and references |
| Outcome unverified | Review pinned evidence and record a host verification conclusion before acceptance |
| Failed verification | Inspect the selected evidence and written conclusion |
| Unknown historical verification | Record a new host verification after checking the evidence |
| Stale attempt | Publish new evidence under the current claim; retain history |
| New verification after acceptance | Wait for the new receipt and accept again |
| Unknown knowledge write | Query receipts before any new update |
| Cross-topic reuse | Direct references are unsupported; a host must establish authorized independent evidence in the destination topic |

See [architecture](../design/07-outcomes.md) for the complete contract.

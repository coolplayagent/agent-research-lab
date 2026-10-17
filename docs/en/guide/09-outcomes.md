# 09 Using outcome collaboration

[Documentation](../00-index.md) · [Guide index](00-index.md) · [简体中文](../../zh-CN/guide/09-outcomes.md)

## Workspace flow

1. Add participants to a conversation and create goals representing independent paths.
2. Open or create a research Topic belonging to that conversation. General collaboration topics need no research CLI configuration.
3. In the topic's outcome section, link an existing goal and record its path name and hypothesis.
4. Publish a conclusion, applicability conditions, evidence at fixed commits, source commit and prompt/policy digests. Publication initially remains unverified.
5. Search by keyword, kind or path. Read full evidence only when relevant. Reference and publish a new outcome, recording why it uses, supports or challenges the source; select another path if appropriate.
6. In system service settings, add a verifier and bind it by topic and outcome kind. New publications then request verification automatically; details also allow explicit requests.
7. After successful verification, the host reviews conditions and receipts and accepts or returns the outcome. Goal acceptance remains a separate workflow.
8. Optionally bind a knowledge service and repository alias. From an accepted outcome, explicitly request an update with the expected target revision. Reconcile unknown writes instead of repeating them.

Revisions use new IDs and preserve prior versions. Withdrawal preserves references and audit history. Archived topics are read-only; removing a group member immediately removes outcome access. Links still require current authorization.

## Agent API

Agents use existing AI-IM bearer grants and work claiming. Publish only while the exact goal, work, attempt and claim are valid. `POST /api/crystal/outcomes` accepts:

| Operation | Fields | Purpose |
| --- | --- | --- |
| `paths` | `topic_id`, `after` | Read paths and hypotheses |
| `search` | `query: {topic_id, goal_id?, kind?, query?, after?, snapshot?}` | At most 32 summaries |
| `get` | `id` | Full evidence and current status |
| `publish` | `input` | Outcome bound to the current execution attempt |
| `history` | `id`, `after` | Paginated audit history |

See [outcome-publication.json](../../../examples/outcome-publication.json). Replace placeholder hashes with actual evidence. `knowledge: null` and `pins.superpod: null` mean unbound. Reuse an ID only for identical retries; changed content requires a new ID. Continue searches with returned `snapshot` and `next_after`; omit snapshot to refresh. Include discovery and verification costs in the execution budget.

A client may use `curl --config "$AI_IM_AUTH_CONFIG" --json @request.json "$AI_IM_URL/api/crystal/outcomes"`. Keep the authentication config in a private runtime directory with mode 0600, outside Git. Credential values must not enter command arguments, outcomes or prompts. Execution adapters do not automatically receive bearer access. Operators configure direct Agents through the existing grant flow; configured coding executors can continue returning results for host publication.

Operator APIs are `GET/POST /api/im/outcomes`, `POST /api/im/outcomes/verify`, `POST /api/im/outcomes/write`, and `POST /api/im/outcomes/reconcile`. Mutations require an operator session, same origin and `X-Crystal-Intent: manage-outcomes`. Verification takes `request_id, outcome_id, verifier_id, configuration_revision`; writes take `request_id, outcome_id, expected_revision, configuration_revision`; reconciliation takes `outcome_id, request_id`. Forged internal receipt commands are rejected.

## Optional services

```sh
bazel build //crates/service-workers:ai-im-outcome-service
bazel-bin/crates/service-workers/ai-im-outcome-service \
  --config /private/services/outcome-service.json \
  --state-dir /private/services/outcome-state \
  --socket /private/services/outcome.sock \
  --max-seconds 43200
```

Give each service a separate private state directory and socket parent. Adapt [outcome-verifier-service.json](../../../examples/outcome-verifier-service.json) or [outcome-knowledge-service.json](../../../examples/outcome-knowledge-service.json) to real local paths. Credentials remain in service environments, referenced by `env_names`. A verifier depends on the separate sandbox service described in [service configuration](03-services.md).

The configured verification command reads `OutcomeServiceRequest::Verify` from stdin and returns `OutcomeVerificationResult` JSON with `id, request_digest, state, summary, evidence`. It must run checks appropriate to the outcome kind and produce pinned verification evidence. Exit success, file presence or matching hashes alone do not prove the conclusion. Passed/failed receipts require evidence; an inconclusive check returns unknown or unavailable. Protocol health does not run the full verification.

Knowledge services accept write and read-only lookup requests. RPC uses `service-protocol` v1 bounded frames and private same-user Unix sockets. Types are defined in `service-api::OutcomeServiceRequest`. These sockets are trusted host interfaces and must not be exposed to untrusted Agent processes.

## Git knowledge repository

1. Prepare an import branch in an existing external repository: `git -C /path/to/knowledge branch ai-im-incoming HEAD`. Do not check it out.
2. Map a service alias such as `team-knowledge` to that repository's absolute path and `refs/heads/ai-im-incoming` in private service configuration.
3. Start the service. Add its matching ID, knowledge kind and protocol v1 in settings, then bind the Topic to the service and alias.
4. Read the current full revision with `git -C /path/to/knowledge rev-parse refs/heads/ai-im-incoming` and supply it when requesting the update.
5. The target branch gains `accepted-outcomes/<outcome-id>.json` and `accepted-outcomes/receipts/<request-id>.json`. The receipt identifies the commit and file. Existing checked-out files remain untouched.
6. The knowledge maintainer reviews, merges and indexes through that repository's existing workflow. SuperPOD bindings retain its knowledge-map and publication rules; this adapter does not update its map automatically.

A changed expected revision returns confirmed failure. Interrupted operations may be unknown. Private service records and target commit records enable receipt reconciliation. Preserve service state for idempotency and recovery; deleting receipts or changing request IDs is not a resolution for unknown effects.

## Troubleshooting

| Situation | Action |
| --- | --- |
| No SuperPOD | Leave knowledge unbound and use publication, discovery and references |
| No verifier | Configure one appropriate to the kind; keep outcomes unverified meanwhile |
| Healthy service, failed outcome | Inspect that request's receipt, evidence and conditions |
| Unknown verification | Diagnose protocol/service and create a new verification request |
| Stale attempt | Publish new evidence under the current claim; retain history |
| New verification after acceptance | Wait for the new receipt and accept again |
| Unknown knowledge write | Query receipts before any new update |
| Cross-topic reuse | Direct references are unsupported; a host must establish authorized independent evidence in the destination topic |

See [architecture](../design/07-outcomes.md) for the complete contract.

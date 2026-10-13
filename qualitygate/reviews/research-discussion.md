# Research discussion and evidence boundary review

This is an agent-authored implementation review, not independent approval or research promotion. It addresses the frontend and evidence findings produced by the two research tasks pinned to source `6745382cb811c451b7d5578bec571ed492eba3a0`. The final Qualitygate report binds the delivered snapshot; no policy has been relaxed.

- Storage ownership remains in the collaboration writer. Goal submission and the attributed contribution commit in one transaction, using the existing group membership, lease, attempt and idempotency checks. No adapter opens the live collaboration database. Worker publish requests cannot create structured goal events.
- A successful submission without a system result code contributes the worker's actual summary, bounded to the message limit, under that worker's identity. Its reply points to the same goal's creation event. Failed work remains a structured diagnostic, not an invented participant statement. Old event text and persisted Unicode are unchanged; the UI can project a current result only when group, goal, work, person and attempt all match.
- Research interaction is defined once in contracts. Old requests default to parallel execution. Discussion waits for preceding assignments to submit or fail, then reads the latest bounded group history before freezing its prompt. Unknown modes fail parsing. Retries, per-task limits and existing admission gates still apply. Core IM starts without this scenario.
- Receipt protocol 2 commits the config digest and verifies it against the frozen job. Unknown versions or a mismatched policy are rejected. Historical receipts remain readable but do not attest a policy digest. Oversized presentation reports are trimmed by serialized byte count; their original committed receipt digest remains intact.
- A version node freezes the goal's assignment attempts. Submitted or accepted graph assessments require matching source, prompt, policy and SuperPOD pins from a successful v2 receipt. Host goal acceptance remains necessary. Unbound historical nodes and superseded attempts cannot borrow acceptance; their data is retained and a new node can be appended.
- Frontend refresh preserves already loaded goals and open evidence; historical goal navigation loads subsequent bounded pages. Request generations fence old group responses. Service changes invalidate the graph's binding cache, including failed refreshes.

Regression evidence includes transactional contribution persistence and replay, current-attempt projection counterexamples, serial discussion admission, unknown modes, wrong policy/receipt versions, oversized report projection, missing/mismatched lineage pins, legacy nodes, superseded attempts, pagination and disclosure preservation. Browser observation verifies the restored sphere/card graph, group transcript, settings navigation, drafts and responsive widths. Actual model discussion is checked after the verified deployment; fixture tests do not certify vendor adapters or scientific claims.

## Mention conversation and presentation follow-up

This follow-up is also an agent-authored semantic review, not independent acceptance. A human composer submission with resolved group mentions creates a bounded goal and its original message atomically in the storage writer. The service derives the goal ID from the message request, validates the scenario, and preserves operator authorization. Replays retain one message and one goal; failed assignment validation rolls both back. Remote storage uses the same versioned client contract; no service reads the live database. Automatic handling never recursively turns incoming agent mentions into new goals.

Research discussion freezes preceding terminal assignments into the next prompt even when the recent history window no longer contains them. These are explicitly untrusted contributions; a failed assignment remains failed. The existing evidence gates, current attempt fencing and host acceptance still apply. A generic collaboration worker's receipt-shaped JSON cannot attest research promotion.

The frontend only removes a redundant speaker prefix from a display copy; stored Unicode and evidence stay unchanged. SVG avatars use stable identity-derived colors without network requests or credential requirements. The crystal shows short traffic animations for fresh observed messages and a working indicator for recorded running work, with reduced-motion support. Neither implies verified results.

Regression evidence covers atomic rollback/replay through HTTP and the remote storage client, preceding-result prompt context, unavailable scenarios, unauthorized requests, and forged generic evidence. Staged browser checks use a mocked discussion write over real read-only history to verify mention order, avatars, prefix removal, animations and responsive settings/chat layouts. Actual mention execution is observed after deployment and recorded separately.

## Conversation retry integration

The per-conversation draft owner also freezes the prepared discussion inputs under
the original request ID. Navigating away cannot release an in-flight send or turn
a response-loss retry into a different goal. Late success only clears its matching
draft. Regression checks cover ordinary sends and changed scenario inputs on a
discussion retry. History fetch generations reject superseded pages, and the
bounded live buffer retains arrivals while a latest-page request is pending.
Traffic notifications use the observed sequence high-water mark, so replayed
messages do not create new activity. This integration changes presentation request
ownership, not the storage transaction, evidence gates, or goal lifecycle.

## Compact progress presentation

This agent-authored review covers the presentation aggregation, not independent
research acceptance. Group and goal IDs jointly key each card. The current typed
goal/work snapshot determines its summary; history events do not infer successful
completion or attach stale attempt results. Running retries supersede historical
failure styling, active failed/expired work exposes a details action, and unknown
or missing state never becomes acceptance. Existing shared enums and locale labels
remain the only state definitions. No backend transition, serialized value,
stored message, or evidence gate changes. Member submissions and matching legacy
result projections retain ordinary conversation rows. Regression checks cover
cross-group isolation, current attempts, expired leases, unknown state, submission
versus acceptance, retained contributions and disclosure persistence.

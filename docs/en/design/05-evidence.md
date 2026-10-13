# 05 Research evidence and version lineage

[Documentation](../00-index.md) · [Section index](00-index.md) · [简体中文](../../zh-CN/design/05-evidence.md)

The main workspace includes research topics and an interactive evolution graph.
A topic references an existing collaboration group. Subjects describe the research
object (repository, agent, tool, model, workflow, dataset, prompt or another type).
Topic and subject edits use optimistic revisions, with stable group/topic ownership.

Each append-only evolution node records a version reference, up to four existing
parents, optional source/prompt/policy/SuperPOD commit pins and an optional group
goal. Parent edges must remain within one subject. Parents already exist before a
child is written, so cycles cannot be introduced. Multiple children represent
population branching; multiple parents represent recombination. Repeating an
identical node request is idempotent; replacing an existing node is rejected.

The graph's assessment state is derived from its linked goal and assignments.
A successful worker submission remains pending host acceptance. A node without a
linked goal remains unassessed. References and declared commit pins are provenance
metadata, not independent promotion evidence; existing research gates are preserved.
Knowledge contents stay in SuperPOD rather than a second local knowledge base.

Host APIs: `GET /api/im/topics`, `GET /api/im/subjects`,
`GET /api/im/evolution`, and `POST /api/im/research` (intent `manage-research`).
Topic/subject lists use stable ID cursors; graph pages use durable sequence cursors.
Additive SQLite schema v5 owns topics, subjects, nodes and parent edges in the
storage service. The graph UI supports subject/topic filters, pagination, zoom,
keyboard node selection and inspection of goal results and evidence.

## Assessment evidence

Research receipts bind source, prompt, policy and SuperPOD. Long reports retain a bounded chat summary and the complete receipt digest. Assessments freeze the linked work attempts; acceptance requires matching evidence and all four bindings plus host acceptance. Older receipts/nodes stay readable, but missing evidence leaves them unassessed; new research appends an immutable assessment under the version node as described below.

`evolution lineage` produces a recomputable receipt from `PromptRegistry`. Its request contains `registry_file` and `study`, including `task_kind`, fixed `policy_version` / `superpod_commit`, host-frozen `expected` bindings, development `observations`, evaluation/expansion candidates and budget. The research inspector shows validated receipts without prompt bodies or trial inputs.

Reports separate a policy’s own performance from descendant aggregates, deduplicate shared descendants and charge incomplete work to the budget. Validation/holdout observations, changed bindings and duplicate trials are rejected. This is HGM-inspired lineage diagnosis, not a reproduction of HGM Thompson sampling. Branches neither create new people nor share mutable experiment memories.

## Independent gates

Registration does not activate a prompt. Promotion requires three task families and repeated paired trials; new-capability claims additionally require held-out transfer and ablation evidence. Each family retains one stable, two specialized and two exploratory policies. Exploration starts at 25%, adjusts after a complete ten-experiment window and remains within 10–50%. Formal rule proposals stay quarantined.

Host-owned expected context and measured observations must match; unknown fields, stale bindings, missing measurements and severe regressions fail. Synthetic examples cannot replace real measurements. Holdouts, gate inputs and receipts remain outside candidate-writable mounts; directory separation alone does not establish OS isolation.

Reviewed reusable public inputs remain at [research/insights.md](../../../research/insights.md), whose remote-default-branch commit and content digest are frozen for new tasks. Complete knowledge and source archives remain in SuperPOD. See [experiments](../guide/05-experiments.md) and [delivery](../guide/06-delivery.md).

## Scenario-neutral iteration coordination

The platform separates collaboration, iteration coordination and scenario execution.
Core collaboration owns identities, groups, messages and goals. The generic
iteration engine owns frozen inputs, rounds, candidate versions, independent
evaluations, host acceptance, optional activation and feedback. Scenario adapters
own domain-specific execution and evidence requirements. The engine has no CLI,
repository, MR, vendor or four-person assumption; it starts without research
configuration.

The generic collaboration scenario accepts external workers. The built-in
`design-review` scenario runs bound agents to propose and independently review
structured plans without a Git checkout, MR or code build. Its results are real
agent submissions; a host records the evidence and accepts a candidate explicitly.
The optional `research` scenario retains read-only CLI investigation, and
`software-evolution` adds the repository-specific implementation and delivery
policy. Four-agent CLI research is one deployment configuration of these adapters.
Researching the platform itself uses a separate run and subject.

`GET /api/im/iterations` accepts `group_id`, `scenario_id`, `after` and `active`
filters and returns full run records. `POST /api/im/iterations` requires the
`manage-crystal` intent and an envelope containing `request_id`, `run_id`,
`revision` and an `action` tagged by `operation`. Creation freezes an `input` with
the goal criteria, scenario parameters, subject, baseline, assignments and budgets.
Pause, resume and stop use the same revision-bound idempotent envelope. Pausing
blocks new claims and rounds while already claimed work retains its bound; stopping
cancels the current round. An activation that has claimed its switching phase must
first settle through host reconciliation; pause and stop are fenced while it is
pending. Candidate, evaluation, acceptance, activation and round
completion are separate recorded operations, each checked against the current
goal and assignment attempts. Goals belonging to an iteration cannot be accepted,
cancelled or retried through the ordinary goal mutation API; use the iteration
controls so its evidence and lifecycle remain consistent.

Repeated research of one source commit now creates separate immutable assessments
under one version node. Each assessment binds its own goal, assignment attempts and
per-work source, prompt, policy and SuperPOD pins. The optional CLI reconciler reads
one bounded page of recorded goals at a time through the storage client, recovering
its host cursor from private state. It creates only references supported by frozen
jobs or matching recorded work evidence; incomplete or inconsistent history is
left unassessed. It does not infer parent edges, invent promotions or copy knowledge
out of SuperPOD. Existing host-edited topic and subject data is preserved.

Group status events refresh both iteration records and visible version assessments,
including changes that do not alter the group catalog. Reconnection fetches current
state again. Request generations fence group switches; lower revisions cannot
replace a newer displayed iteration. Version details link recent assessments back
to their goals and evidence.

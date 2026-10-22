# 05 Collaboration experiments and policy evolution

[Documentation](../00-index.md) · [Section index](00-index.md) · [简体中文](../../zh-CN/guide/05-experiments.md)

## Prompt registration and gates

Policies retain immutable parents, role, task families, reasons and failure conditions. Registration does not activate or promote a policy; assess applicability and select a version explicitly.

```sh
mkdir -p .lab/evolution
bazel-bin/outcome-hub-cli evolution prompt-register \
  examples/evolution/prompt-register.json --output .lab/evolution/prompts.json
```

Runtime reads `state_dir/evolution/prompts.json`; adjust the output to your private state. Other actions are `prompt-portfolio`, `evaluate`, `novelty`, `exploration`, `rule-proposal` and `holdout-validate`. See [typed examples and path conventions](../../../examples/evolution/README.en.md).

Gates compare separately pinned host context with actual observations and reject unknown fields, stale bindings, missing measurements and severe regressions. Process success or report claims cannot replace independent evaluation. The host supplies real measurements; repository observations are synthetic fixtures.

Promotion requires three task families and repeated paired experiments, comparing success and measured cost. New-capability claims additionally require held-out transfer and ablation evidence. Each family retains one stable, two specialized and two exploratory policies. Exploration starts at 25%, adjusts after complete ten-experiment windows and remains within 10–50%. Rule proposals remain quarantined without self-approval.

Holdouts belong under `state_dir/private`, outside SuperPOD, shared memory and optimizer worktrees. Host-owned gate inputs/context/receipts stay outside candidate-writable mounts; different directories alone do not provide isolation.

## Eight-participant collaboration pilot

This protocol studies AI-SDLC and long-running agent collaboration using one fixed commit of the existing SuperPOD repository. Plans, raw output and evaluation stay private; the host verifies durable knowledge before writing it back to SuperPOD.

One instance has two arms, eight fixed slots per arm and two rounds: 32 bounded agent calls (Codex task attempts), not provider API requests. One call may contain multiple model turns/tools/retries. Matched slots share model, perspective, prompt version, timeout and claim limit. Round one is independent. In round two, baseline reads its own draft; candidate reads its own draft and the next two peers on a ring, seeking counterexamples before revision. Retain dissent without forcing consensus. The runtime remains the scheduling/side-effect authority.

Both arms use `Task.dependencies`; only workflow-confirmed, correctly bound reports enter as untrusted evidence. Agents do not write to each other or share mutable memory. All tasks are read-only, `use_memory=false`, `next_tasks=[]`, without the continuation-capable `implement` role. If a third model is needed, the host adds a read-only `evaluate` role privately; the harness neither changes configuration nor starts tasks.

Eight interdependent slots are not eight independent statistical samples. The 32 calls form one team-level pilot and do not prove eight simultaneous processes; concurrency needs separate interval evidence. Equal calls/time limits do not imply equal tokens/cost. Prealternate `first_arm` across later independent instances to address order/latency effects.

## Plan, register, run and collect

Let workers finish naturally and stop the controller before selecting the private configuration, common SuperPOD source and budget. Ordinary enqueue does not stop another controller discovering tasks; batch enqueue is not a global pause. Reuse isolation/state boundaries so a new state directory does not expose existing private data. Example model mappings and the 300-second timeout must match the real controller.

```sh
outcome-hub-cli --config /ABS/pilot.toml collaboration plan examples/collaboration/pilot.json --output /ABS/STATE/private/collaboration/pilot-001
```

`plan` checks current sources/skills online and freezes bindings, perspectives, questions, primary hypothesis and DAG. It registers parent-linked immutable prompts without selecting/promoting them. Replay is idempotent; changed source/config/request needs a new experiment ID. Each task must read the declared common SuperPOD files and cite fixed commit/path, file SHA-256, line range and short exact text. Missing resolvable common-knowledge citations make the protocol result invalid.

Use `collaboration enqueue MANIFEST --output /ABS/STATE/private/pilot-enqueued.json` to register all 32 tasks, then run the existing controller. Batch enqueue performs one freshness capture, checks the frozen plan/config/prompts and all existing tasks, then registers in manifest order through normal enqueue. Matching tasks retain original inputs/digests. Replay fills missing entries; stale baselines or mismatches fail. `worker_launch_requested=false` describes this command only; launch rechecks freshness. The batch is not transactional: races/I/O failure may leave a valid prefix, which replay checks before continuing.

Individual `enqueue TASK.json` remains supported; register round-one tasks first. Extract `tasks[].task.id` as a JSON string array into a regular file under the same `state_dir/private`, then run non-continuously with `run --max-seconds 3600 --task-ids-file /ABS/STATE/private/pilot-ids.json`, without `--seed`. The file must contain 1..256 unique enqueued IDs and every dependency; tasks must match the current config digest. It is capped at 64 KiB and must not traverse symlinks. Continuous mode seeds idle research even without `--seed`, so cannot run this fixed-budget pilot.

Retain timeout/failure/invalid output in the planned denominator. Do not select successes or silently retry one arm. Each planned task has `max_attempts=1`, enforced for automatic, recovery and manual retries; unspecified historical tasks retain three attempts. A retry needs a new explained instance. The real watchdog enforces timeouts.

```sh
outcome-hub-cli --config /ABS/pilot.toml collaboration collect /ABS/STATE/private/collaboration/pilot-001/manifest.json --output /ABS/STATE/private/collaboration/pilot-001/collection-001.json
outcome-hub-cli --config /ABS/pilot.toml collaboration collect /ABS/STATE/private/collaboration/pilot-001/manifest.json --adjudications /ABS/STATE/private/collaboration/pilot-001/host-grades.json --output /ABS/STATE/private/collaboration/pilot-001/adjudicated-001.json
```

Explicit scope filters scheduling and post-success work and skips the global automation outbox. Existing nonempty follow-ups remain pending; scoped tasks proposing more work record `deferred_task_scope` and Pending, to be processed later without the scope. Global old-process cleanup, leases and side-effect recovery still run. The `task_scope` result contains input digest, selected IDs and deferred runs; global status fields still describe the full state directory.

Each collection writes a new immutable snapshot; absent/nonterminal tasks remain incomplete. It checks local receipts against workflow authority, task/DAG, config, model, prompt, source, skills and SuperPOD bindings. Stale/forged receipts contribute no claims. Historical collection cannot bypass current-version promotion checks.

## Claims and independent adjudication

New plans use `collaboration-pilot-v2`. Each `findings` string carries a JSON [claim card](../../../examples/collaboration/claim-card.json). Local `id` uses 1..100 ASCII letters, digits, underscores or hyphens, such as `c1`, never task ID or `#`. The host forms `task-id#c1`; only cross-card `counterexample_to` and `revises` use full references. References may target provided dependencies only, and revisions only the author’s own first draft. Changes, counterexamples and knowledge updates are proposals.

Frozen `collaboration-pilot-v1` plans retain original text/DAG/digests and v1 collection. v2 neither rewrites old plans/reports nor accepts unsafe old IDs/additional attempts. Use a new experiment ID after protocol changes; unknown protocols fail.

`scheduled_calls` and other `*_calls` count agent calls, not provider requests. The collector distinguishes:

- Citation resolution: pinned Git file digest, lines and short text match; matching text does not prove support.
- Literal duplication: whitespace/case-normalized final claims. Shared citations alone are not duplicate work; semantics require host review.
- Counterexamples/revisions: proposed relations count as valid only after independent adjudication.
- Cost diagnostics: observed Codex `turn.completed` usage from every registered call, including failed/invalid reports. Missing/unexecuted stays null. Worker-writable logs, even hashed, are not independent billing evidence or automatic efficiency-promotion proof. Time limits are not measured duration.

Host [adjudications](../../../examples/collaboration/adjudications.json) must live under `state_dir/private`, bind plan/claim/receipt digests and include independent sources/reasons. Prefer blinding arm labels. Compare deduplicated independently supported conclusions and valid correction targets, reporting miscitation, new errors, failures and missing evidence. The host sets `introduced_error` by comparison with round one; unassessed remains null, with assessed counts reported. Repeated corrections to one draft contribute only one `unique_counterexample_targets`. More words/criticism or self-reported change do not prove improvement.

`knowledge_update` names an existing SuperPOD file. The collector emits `pending_host_review`; the host reviews visibility/evidence/conflicts/nodes and uses the existing publication path with one index writer. Agents do not push concurrently or create a replacement knowledge base.

## Follow-up experiments

Use real traces to diagnose protocol gaps before freezing instances. One pilot produces neither success labels nor `TrialObservation`, and does not automatically call `evolution evaluate/promote`. Formal promotion still requires at least three task families, three independent team-paired trials per family, the same formal policy/fixed knowledge version and existing gain/severe-regression thresholds. Novelty needs held-out families and ablations.

Ablate peer evidence, criticism and perspective separately while holding other models/budgets fixed. Register parent-linked candidates and change one prespecified role policy at a time; diagnose development instances before isolated independent evaluation. Rebaseline after model/source updates. Synthetic protocol regressions prove fields/dependencies/collection, not improved research or emergent ability.

## Background references

These retained design references inform the protocol, not evidence of this project's comparative performance:

- [OpenAI orchestration and handoffs](https://developers.openai.com/api/docs/guides/agents/orchestration): bounded specialists and final responsibility; the host owns scheduling and acceptance here.
- [OpenAI agent evaluations](https://developers.openai.com/api/docs/guides/agent-evals): trace diagnosis and reproducible evaluation; this implementation uses local Rust formats, not hosted evals/prompts.
- [Anthropic multi-agent research](https://www.anthropic.com/engineering/multi-agent-research-system): explicit goals, tools, bounds and citation checks; their comparison does not establish an eight-agent benefit here.
- [Anthropic long-running harness design](https://www.anthropic.com/engineering/harness-design-long-running-apps): testable contracts, separate generation/evaluation, ablations and remeasurement as models change.

## Policy lineage receipts

`evolution lineage REQUEST.json --output STATE/evolution/lineage.json` uses `PromptRegistry` to produce a recomputable receipt. The request has `registry_file` and `study`: `task_kind`, fixed `policy_version`/`superpod_commit`, frozen `expected` bindings, development `observations`, evaluation/expansion candidates and budget. The research inspector's population-evolution settings show validated receipts without prompt bodies or trial inputs.

Reports distinguish policy performance from descendant aggregates, deduplicate shared descendants and charge unfinished work. Validation/holdout observations, changed bindings and repeated trials fail. This is HGM-inspired diagnosis, not HGM Thompson sampling reproduction. Activation still requires evaluation, holdout, promotion, publication and installation gates; branches do not create identities or share mutable experiment memory.

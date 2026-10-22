# 05 Research evidence and coordination

[Documentation](../00-index.md) · [Section index](00-index.md) · [简体中文](../../zh-CN/design/05-evidence.md)

Topics reference existing collaboration groups. Subjects describe external repositories, agents, tools, models, workflows, datasets or prompts. Topic and subject edits use optimistic revisions and stable ownership. Knowledge content remains in the bound external repository.

The host exposes `GET /api/v1/collaboration/topics`, `GET /api/v1/collaboration/subjects`, and `POST /api/v1/collaboration/coordination` with `manage-research` intent. Coordination writes support topics and subjects only.

Version evolution and improvement iterations have been removed from the UI, HTTP API, storage protocol and research CLI integration. There is no automatic iteration scheduler, version synchronizer or software-evolution activation command. Schema 9 removes their stored history through offline migration.

Ordinary goals still bind every work result to its current attempt. Research receipts retain source, prompt, policy and required SuperPOD pins; a worker submission is not host acceptance. Generic collaboration and design review remain optional scenario choices; research execution keeps its existing admission and evidence gates.

## Independent gates

Registration does not activate a prompt. Promotion requires three task families and repeated paired trials; new-capability claims additionally require held-out transfer and ablation evidence. Each family retains one stable, two specialized and two exploratory policies. Exploration starts at 25%, adjusts after a complete ten-experiment window and remains within 10–50%. Formal rule proposals stay quarantined.

Host-owned expected context and measured observations must match; unknown fields, stale bindings, missing measurements and severe regressions fail. Synthetic examples cannot replace real measurements. Holdouts, gate inputs and receipts remain outside candidate-writable mounts; directory separation alone does not establish OS isolation.

Reviewed reusable inputs belong to the bound external knowledge repository at `knowledge/software/ai-sdlc/insights.md`. New tasks require that committed regular file and pin its repository revision and content digest. Missing inputs block admission; no empty fallback is used. Preserve original historical evidence and enqueue a new experiment when its source binding is obsolete.

# 03 Services and Agent integration

[Documentation](../00-index.md) · [Section index](00-index.md) · [简体中文](../../zh-CN/guide/03-services.md)

## Core service settings

In settings, configure private Unix sockets and enablement for storage, sandbox and executor services, then executor-to-sandbox dependencies, adapter profiles and person bindings. Updates use revisions; resolve invalid references/dependencies first. Leaving unsaved forms prompts a warning. Storage and worker packages include service templates with bounded processes, memory and output.

Development can use an explicitly separate embedded store. One storage service owns the production database. Prepare target data before changing its endpoint, then stop/restart the core; an endpoint change must not silently replace a live store. See [operations](07-operations.md) for deployment and backups.

Claude Code, Codex, DeepSeek Harness and Pi profiles start disabled. Supply a wrapper command array implementing `json_stdio_v1`, then enable and bind it. A vendor CLI name alone is not compatibility evidence. Health probes verify protocol and capabilities, and the sandbox runs a real isolated command; model tasks require separate verification.

Keep secret values in the service environment. Profiles list allowed variable names only; do not expose values through the UI, responses or command arguments. Optional mounts are read-only, network access is explicit and every run gets a disposable workspace. See the [service protocols](../design/03-services.md) for frames, requests, output and attempt fencing.

## External workers

Identity creation and host-managed execution need no personal token in the UI. A trusted external worker obtains a short-lived grant through the host management API: returned once, stored only as a digest and valid at most 24 hours. Host writes require exact Origin, JSON Content-Type and `X-Crystal-Intent: manage-crystal`.

Workers claim their own assignments, obey the returned deadline and submit under the same claim and attempt. After submission, inspect host acceptance in the group; stdout or health status is not completion. See the [goal and messaging APIs](../design/02-collaboration.md).

## Optional research backends

Research uses the separate local `json_process` result-file protocol. [examples/backend.toml](../../../examples/backend.toml) shows `agent_backends`, role selection with `role_backends`, task defaults in `[models]` and optional per-backend lists in `[backend_models]`. Adding a model does not create a person; omitting a list permits the existing role defaults.

```sh
outcome-hub-cli --config local.toml agents backends
outcome-hub-cli --config local.toml doctor --probe-models
```

Model probes may consume provider quota. A generic bridge probe only verifies the local JSON contract. Codex model-response probes and actual task results are separate evidence. Unknown models or insufficient capabilities fail explicitly without silently switching models.

## Proposal boards and simulation

Enable shared proposals for ordinary research:

```toml
[multi_agent]
shared_research = true
```

This freezes new task identities; historical tasks retain their inputs, and formal collaboration pilots/holdouts do not join automatically.

```sh
outcome-hub-cli --config local.toml agents boards --shard 0 --limit 32
outcome-hub-cli --config local.toml agents view --cohort SHA256
outcome-hub-cli --config local.toml agents simulate --agent-count 1000
outcome-hub-cli --config local.toml agents simulate --agent-count 10000
```

Lists use shards/cursors. `agents close` prevents new membership; `agents prune` requires closure and expiry, with required evidence archived first. Tombstones remain; neither operation restores quotas or publishes knowledge. Simulations test logical routing/resource bounds, not 10,000 model processes or research-quality gains. Actual execution remains limited to eight workers.

## Scenario services

System settings include a **Scenario services** page. Optional `configuration.scenarios`
entries specify stable ID, private Unix endpoint, enabled state, protocol version,
declared capabilities and dependencies on configured services. Existing agent
bindings select execution adapters independently. The versioned scenario protocol
supports probe, goal validation and revision/attempt-bound reconciliation. A
reconciliation request retains the same identity for retries; external effects
must reconcile uncertain responses before being repeated. Configuration, protocol
health and actual task results are shown separately. Credentials stay in service
environments and are not returned in configuration responses.

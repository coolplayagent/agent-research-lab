# 03 Services and Agent integration

[Documentation](../00-index.md) · [Section index](00-index.md) · [简体中文](../../zh-CN/guide/03-services.md)

## Bind an Agent

System settings opens **Agent bindings**. Create a digital person, select Claude Code, Codex, DeepSeek Harness or Pi, then choose **Validate and bind**. The installed Agent must already be authenticated. The validation performs a model request and may consume provider quota; its progress and failure stage appear on this page. Only successful validation enables the adapter and saves binding evidence. Creating an identity or saving an adapter template does not invoke a model.

Use the Linux `//:local-package` or the Windows package for local execution. Keep `outcome-hub`, `outcome-hub-services` and `outcome-hub-agent` together (with `.exe` on Windows). A fresh workspace uses a separate embedded development store and starts its managed executor and sandbox on demand. **Execution status → Prepare local runtime** checks the local services without invoking a model. Enabled adapters restore their local services on the next host start. Host shutdown cleans up the processes it owns. Core chat can still start without these optional executables; Agent validation then reports the missing runtime.

## Advanced deployment

The collapsed **Advanced settings** section contains service connections, execution adapters, scenario services and storage. Existing deep links still open their page. Leaving unsaved forms prompts a warning.

| Component | Responsibility |
| --- | --- |
| Executor service | Accept the versioned execution protocol and use its configured sandbox. Its service card lists adapters that reference it. |
| Execution adapter | Select an Agent program through the shared JSON protocol and associate it with an executor. |
| Sandbox service | Run commands with bounded lifetime/output and disposable state. |
| Storage service | Own the production collaboration database behind its API. |

Existing service IDs and roles are fixed when editing; create a separate connection to use another role. Only enabled sandboxes are eligible executor dependencies. Configuration updates validate references and revisions atomically.

Built-in Agent profiles start disabled. Saving one stores a template; **Validate and bind** generates its bridge command. Changing its executor or time limit requires revalidation. Custom adapters retain the advanced `json_stdio_v1` command-array editor and require a command before being enabled. A configured profile or healthy service alone is not model/task compatibility evidence.

Service management is explicit: `local_managed` starts packaged workers at their private workspace endpoints; `external` connects to separately managed services. Existing configurations without this field remain external, including disabled entries and custom endpoints. Installing a new package never takes over them. To use an existing external setup, start its configured services before validation.

Development uses an explicitly separate embedded store. One storage service owns the production database. Prepare target data before changing its endpoint, then stop/restart the core; an endpoint change must not silently replace a live store. See [operations](07-operations.md) for deployment and backups.

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

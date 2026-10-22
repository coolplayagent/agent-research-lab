# 03 Services and Agent integration

[Documentation](../00-index.md) · [Section index](00-index.md) · [简体中文](../../zh-CN/guide/03-services.md)

## Bind an Agent

System settings opens **Agent bindings**. Create a digital person, select Claude Code, Codex, DeepSeek Harness or Pi, then choose **Validate and bind**. The installed Agent must already be authenticated. The validation performs a model request and may consume provider quota; its progress and failure stage appear on this page. Only successful validation enables the adapter and saves binding evidence. Creating an identity or saving an adapter template does not invoke a model.

Each digital person retains only its latest validation process and private diagnostic. Starting another validation removes the previous process log, including after restart; the active binding changes only after a successful validation. A bounded journal of request fingerprints prevents obsolete requests from repeating model calls. Use a new request ID for a deliberate retry. Failed execution output is bounded and redacted in the private `binding-<request-id>.diagnostic.json` file; HTTP responses contain stable codes and the diagnostic identifier, not stderr.

Execution services forward configured proxy variables by name. Codex native tool hosts are mounted read-only and included in installation evidence. Linux sandbox processes have a 16 GiB virtual address ceiling to accommodate the tool host reservation; this is a virtual address limit, not a resident-memory guarantee. Existing lifetime, file, output and challenge checks still apply.

Install the Linux `//:local-package` or Windows `//:windows-package`. Keep `outcome-hub`, `outcome-hub-storage`, `outcome-hub-services`, `outcome-hub-agent` and `outcome-hub-knowledge` together (with `.exe` on Windows). New workspaces automatically start a separate storage service. Execution, sandbox and knowledge services start on demand without endpoint entry. **Local environment → Prepare** starts and checks the execution services without invoking a model. First binding or execution also prepares them; enabled adapters restore services after restart. Install and authenticate your Agent CLI separately.

Existing `services.json`, external deployments and legacy `crystal` directories retain their storage choice without database migration. Storage recovery makes at most three attempts against the same data; missing data or failed recovery is an error. `serve --embedded-development` explicitly selects the embedded development store for a fresh workspace. Missing execution or knowledge binaries do not prevent core chat; missing storage on a new normal installation prevents startup and requires restoring the complete package.

Create a digital person directly in Agent bindings; the new person is selected while your Agent, program and model choices are preserved. Knowledge repositories and group/goal defaults live under **Settings → Knowledge**. Topic-level service bindings have been retired.

## Add an optional knowledge store

Open **Settings → Knowledge**, choose **Add knowledge store**, and select Git or a local directory in the same form.
Enter a name and an existing absolute path on the host. The whole selected
location is used by default; advanced options can restrict it to a relative content
subdirectory. New stores are read-only. Git reads detect the current branch; detached
HEAD or an empty repository requires preparing a usable branch first. Write access
requires an explicit existing publication branch that is not checked out. Adding a
store does not clone, create branches, commit, publish, or create a default binding.

The Linux and Windows complete packages include the optional `outcome-hub-knowledge` executable.
The host starts it on demand with private state and cleans it up on shutdown. Core
collaboration still starts without it. Knowledge settings accept only this host-managed
service; external knowledge service connections are no longer configurable or usable.
On upgrade, the registry backs up its configuration and removes external knowledge
connections and their current default bindings. Historical outcomes, receipts,
local connections and knowledge files are preserved.
Cards show the saved path, type, access and enabled state, with edit, disable and delete actions.
Default bindings select an enabled store by name. Unbind references before deleting its configuration; knowledge files remain in place. Legacy
research adapters retain their original evidence pins and gates.

Directory targets exclude `.git` and `.outcomehub` at every level and apply only
`.gitignore` files inside the selected content root. Machine-wide ignore files
are not read. Large attachments participate in version calculation through a
64 KiB streaming SHA-256 buffer; fetching text still has a 1 MiB response limit.
The whole operation is bounded to 65,536 directory entries, 8 GiB of input across
all scan passes, 32 directory levels and 45 seconds. `.gitignore` files have at
most 4096 lines of 4096 bytes each and the existing 1 MiB document bound. SHA-256
is optimized in development builds to keep large snapshots within this deadline.
The same exclusions apply to reads and publication destinations. Symlinks,
special files, traversal and changes during scanning fail explicitly.

Directory revisions use `directory-v2:` to distinguish the content/ignore policy
from the old unfiltered snapshot. Old revision preconditions require a new preview;
historical receipts are not rewritten. Errors contain stable codes and structured
path/size/limit parameters, rendered by frontend locale resources.


## Advanced deployment

The collapsed **Advanced settings** section contains external service connections, execution adapters and storage. Existing deep links still open their page. Leaving unsaved forms prompts a warning.

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

## Retired service configuration

External scenario services, verifier services and topic-level service bindings are retired. If private `services.json` still contains nonempty `scenarios`, `outcome_bindings` or a verifier-kind `outcome_services` entry, startup fails without rewriting the file. Back up and remove those entries before restart; historical outcomes and goals remain intact. Built-in collaboration and design review scenarios remain available.

# Local Agent settings and runtime review

Baseline: `361445447b5808d4e290fc76f005ca5a32aa0dba` (v0.2.1).
This is an agent-authored implementation and semantic review, not independent
approval or authenticated vendor acceptance. Final delivery requires the full
snapshot-bound Qualitygate and repository tests.

## Ownership

| Owner | Responsibility |
| --- | --- |
| Collaboration host | Restore explicitly managed execution workers for enabled adapters; observe shutdown and drop only owned process handles. |
| Service registry | Validate service references, management mode, private endpoint location and configuration revision; persist configuration and server-owned evidence. |
| Executor / sandbox | Separate services and private state, accessed through their existing versioned protocol; no collaboration database handles. |
| Binding workflow | Generate native bridge commands and validate the existing isolated task challenge before enabling or binding an adapter. |
| Frontend settings modules | Daily binding/status presentation and advanced deployment editors; shared enums come from generated contracts and Chinese text from locale catalogs. |

Production storage ownership is unchanged. The development host still uses the
same collaboration client contract with its separate embedded store. The
registry's active-storage snapshot and restart-required indication are retained;
changing a configured endpoint never replaces the active store in place.

## Configuration and evidence review

- `ServiceManagement` is a separate shared contract. Missing mode deserializes
  as `external` and remains omitted when serialized, preserving existing wire
  fixtures. Fresh defaults alone select `local_managed` for executor and sandbox.
  External endpoints and disabled services are never automatically converted.
- Service-kind changes for an existing identity are rejected under the same
  lock as revision comparison and persistence. A rejected save changes neither
  revision nor the durable configuration. The editor also fixes ID and role and
  offers only enabled sandbox dependencies.
- Native template saves are disabled and need no hand-written protocol command.
  Direct settings writes cannot newly enable or modify an enabled native adapter.
  They also cannot create a new binding or forge binding evidence. Adapter edits
  invalidate evidence using the existing adapter digest. Custom protocol adapters
  still require a valid command when enabled.
- The binding workflow retains its durable request identity, cancellation,
  deadline, configuration compare-and-swap, program/configuration pins and actual
  sandbox challenge. Runtime preparation does not enable adapters, mutate
  configuration or create evidence. Service health, configuration and validated
  bindings remain separate from goal results and host acceptance.
- New settings failures use stable contract codes with frontend translations.
  Private runtime error text is not included in the public settings error body.

## Process and packaging review

- Preparation selects only the requested executor and its declared sandbox.
  External services are probed through their existing endpoints. Managed workers
  must use their own workspace's private endpoint and the adjacent packaged
  executable. Repeated preparation reuses owned workers.
- Worker processes use Unix process groups or Windows kill-on-close Job Objects,
  a 12-hour lifetime, bounded output and private per-launch logs. Configuration
  changes and periodic polling drop disabled, changed or exited workers. Failed
  spawns and owned shutdown remove their launch logs. No PID discovery or blanket
  kill is used for cleanup.
- Fresh core startup launches no worker until preparation or binding is requested.
  A restart restores local services for enabled adapters without repeating model
  validation. A missing execution package does not prevent core chat startup.
- Linux `local-package` keeps the host, worker and bridge executables adjacent and
  executable. The standalone core package retains its optional-dependency
  exclusion. Windows retains its existing adjacent executable layout.
- Linux runtime read-only mounts are installed after private `/tmp` and home
  creation so those private filesystems cannot hide an installed Agent. The
  command still runs with the same namespace, network, output and lifetime
  boundaries. The regression executes a mounted binary and checks that writing
  to its mounted source remains denied.

## Verification and limits

Registry tests cover old external configuration, immutable service roles, rejected
native enablement, missing packaged workers, early process exit and owned cleanup.
The packaged Linux smoke test exercises all four deterministic Agent peers,
idempotent preparation/binding, restart restoration, disablement and shutdown.
Windows CI runs native service/registry/isolation tests and the existing packaged
four-Agent and Node-launcher protocol fixtures. These tests use deterministic
peers and do not establish authenticated vendor model acceptance for this change.

Frontend behavior tests execute the actual form modules, including native template
save, custom command validation and service role/dependency selection. Browser
checks cover the default route, collapsed advanced settings, deep links, Codex
save and a narrow viewport. Full Bazel checks include wire compatibility,
generated-contract/assets drift, locale integrity and the presentation boundary.
Research/SuperPOD evidence gates and goal-attempt fencing are unchanged.

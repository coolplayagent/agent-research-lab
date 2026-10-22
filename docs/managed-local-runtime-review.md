# Managed local runtime review

This is a local implementation and architecture review by Codex, not independent approval.

## Ownership and compatibility

- Initialization reads an existing `private/services.json` without changing its storage selection. A legacy `crystal` directory keeps the embedded development adapter. Only a fresh workspace selects managed storage; `--embedded-development` is an explicit fresh-workspace alternative. Missing configuration alongside managed storage metadata is an error.
- The host persists the new configuration before spawning storage and waits for its protocol handshake before registering the operator or accepting HTTP requests. The database is opened only by the storage process. The host uses the existing collaboration client API, and no service shares its mutable store handles.
- Storage holds a separate owner lock. Its frozen service identity and data directory remain active even when settings request another storage endpoint. Such a request still requires a deliberate restart against prepared data. Execution cleanup never stops or replaces the live store.
- A durable ownership marker prevents recovery from recreating a missing database. Recovery is limited to three attempts with backoff, always using the original endpoint and directory. A conflicting endpoint is not adopted. There is no fallback to a new embedded store.
- Existing external endpoints and disabled services retain their management mode. The settings API cannot change a service's management mode or invent a managed service. Managed addresses are generated under private state; advanced external connections retain endpoint editing and revision checks.

## Readiness and user interaction

- `LocalRuntimeComponent`, `LocalRuntimeState` and `LocalRuntimeErrorCode` are defined in Rust contracts and generated for TypeScript. The additive `local_runtime` response exposes purpose/readiness, package availability and structured errors without paths or credentials. Existing configuration and health response fields remain available for advanced settings.
- Execution preparation uses the existing revision-fenced endpoint and never invokes a model. First binding or execution also prepares its services. A successful protocol probe never manufactures binding evidence or task success. Agent installation and authentication remain explicit user prerequisites.
- Knowledge is started only when requested. Its configuration and content operations remain behind its service protocol. External contents stay in the selected repository or directory. Existing evidence pins, publication revision checks, journal reconciliation and host acceptance are preserved.

## Windows knowledge boundary

- Linux retains descriptor-relative traversal. Windows pins every traversed directory using opened handles that deny write/delete sharing, and rejects reparse points on the actual opened handle. Reads deny write/delete sharing until bounded reads and metadata checks finish. Junction traversal and ancestor replacement have native regression tests.
- Directory revisions retain the `directory-v2` format and use forward-slash logical paths. Ignore rules, document/snapshot/entry/depth/time bounds, read-only targets and publication receipts remain enforced on both platforms.
- Git runs with an explicit environment, empty global configuration, disabled hooks and signing, a disposable index, bounded output and a deadline. Windows uses the existing Job Object process owner, which contains descendants on cancellation/drop. Git for Windows is needed only for Git targets; directory targets need no Git installation.

## Verification ownership

`//:local-runtime-smoke` runs the real Linux package, checks storage crash recovery and persisted identities, validates four deterministic Agent peers, and exercises restart and disabling execution without stopping storage. `//:local-knowledge-smoke` checks the packaged knowledge lifecycle and target persistence. Registry tests cover initialization, missing data, exclusive ownership and bounded recovery. Frontend tests check readiness cards, revision submission, pending state, errors and retry.

Windows CI builds the native complete package and runs storage, protocol, registry, directory, Git and knowledge tests, plus packaged core, knowledge and binding smoke scripts. Cross compilation alone is not native runtime evidence. Repository-wide Bazel tests and full Qualitygate must pass for the delivered snapshot. These deterministic checks do not establish authenticated third-party model acceptance.

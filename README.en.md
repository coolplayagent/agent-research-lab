# OutcomeHub

[中文](README.md) · [Documentation](docs/zh-CN/00-index.md)

**OutcomeHub is an outcome-based multi-agent work system.** People and agents work toward shared goals, produce versioned outcomes with evidence, obtain host acceptance, and reuse those outcomes in subsequent work.

Messaging carries communication. Agent and subagent execution performs tasks. Outcome collaboration records what was delivered, its provenance, how it was assessed, and which later outcomes depend on it. Execution success, verification, host acceptance, and external publication are separate lifecycles.

## Capabilities

- Identities, groups, goals, assignments, bounded execution attempts and iterative work.
- Immutable outcome versions, evidence pins, reference graphs, independent verification and acceptance.
- Replaceable agent execution and sandbox services using versioned protocols.
- Multiple external Git or directory knowledge stores configured through the CLI or Console; publication previews, revision checks and durable receipts.
- Optional research and software delivery scenarios. Core collaboration starts without scenario configuration.
- Separate application, domain, API, client, storage and adapter modules.

SuperPOD can be configured as an ordinary external knowledge target. The core has no built-in SuperPOD repository address or content layout. Knowledge contents stay outside this repository; credentials and raw runs stay out of Git.

## Start

Rust implementation; Bazel is the build, test and packaging entrypoint.

```bash
bazel build //:outcome-hub //:outcome-hub-cli //:outcome-hub-package
bazel run //:outcome-hub -- serve --state-dir /absolute/path/to/state --listen 127.0.0.1:4877
```

Use `outcome-hub link` with the same state directory to retrieve the operator access link. Existing state locations are preserved; renaming never switches a live database.

Applications live in `apps/outcome-hub`, `apps/cli`, and `apps/console`. Build the optional services with `//:adapter-package`, or the full distribution with `//:package`. Only canonical OutcomeHub commands are packaged; use the separate migration tool before opening retired state formats.

## Documentation

- [Architecture and ownership](docs/outcomehub-architecture.md)
- [External knowledge stores](docs/external-knowledge.md)
- [Migration guide](docs/outcomehub-migration.md)
- [Quick start](docs/zh-CN/guide/01-quick-start.md)

Run `bazel test //...` and the repository's full Qualitygate against the final delivery snapshot.

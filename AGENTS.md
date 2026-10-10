# Agent Research Lab

Implement in Rust; Bazel is the build, test and packaging entrypoint.
Knowledge belongs to the existing SuperPOD repository, not a duplicate local knowledge base.
Keep credentials, raw runs, private holdouts and machine configuration out of Git.
Pin evidence to source, prompt, policy and SuperPOD commits. Never weaken a gate to promote a candidate.
Use isolated worktrees and bounded processes. Do not share mutable experiment memories.
Run the repository tests and full Qualitygate on the final snapshot before delivery.

## API and presentation boundaries

AI-IM owns identities, messaging, groups and goals. Research CLI execution is an
optional scenario adapter; core collaboration must start without research
configuration. Temporary conversations follow a fixed audience, like a
multi-person direct message, and have no expiry time.

Rust source files, including comments and test code, must not contain Chinese
characters. Server-owned prompts, logs, diagnostics and default data use English.
User-provided Unicode content and existing persisted data must remain intact;
multilingual compatibility data belongs in external test fixtures.
The server provides language-neutral APIs with stable codes and structured data.
Chinese presentation text belongs exclusively in frontend locale resources;
components must use translation keys and parameters, including static HTML,
validation, status labels and system-event rendering. Do not translate user data.

## Shared state contracts

Define each state domain once in the shared `contracts` crate and reuse its typed
enums across services and adapters. Preserve existing serialized values during
refactors. Do not redeclare state enums, string unions or state-label dictionaries
inside feature modules, or branch on ad hoc state string literals. Generate the
frontend contract from that Rust source; verify generated output for drift.
Keep different lifecycles as separate named enums, even when some wire values
coincide. Parse external protocol states at their boundary and handle unknown
values explicitly; unknown state must never imply successful completion.

## Module and verification discipline

Keep frontend behavior in focused TypeScript modules. Locale catalogs and shared
contracts must remain separate from business behavior; generated assets are
reproducible through Bazel. Include regression checks for API wire compatibility,
locale key/parameter integrity and the server/presentation boundary. Run the
repository tests and full Qualitygate against the final snapshot before deploying.
Architectural ownership and semantic state-machine changes also require review;
a text scan alone is not evidence that those properties hold.

## Replaceable service boundaries

Coding agents are replaceable execution adapters. Claude Code, Codex, DeepSeek
Harness and Pi use the same versioned protocol; never make core messaging or goals
depend on one vendor's CLI, credentials or native output format. Configuration is
not compatibility evidence: distinguish service health, configured adapters and
actual task results. Fence every execution result to its current goal attempt.

Production collaboration storage owns its durable database in one service. Other
services use its API and event streams, never open that service's live database or
share mutable handles. An embedded development adapter must use the same client
contract and an explicitly separate local store. Lightweight sandbox execution is a separate, replaceable service with
bounded lifetimes, output, mounts and disposable per-run state. Keep credentials
in service environments and refer to them by name; do not expose values in the UI,
configuration responses or command-line arguments.

System settings must manage service endpoints, enabled state, protocol health,
dependencies and Agent bindings. Validate references and configuration revisions.
Storage endpoint changes require a deliberate restart against prepared data;
never silently replace a live store. Preserve the existing research evidence gates.

## Research coordination and lineage

Keep research topics and subject/version lineage as coordination metadata linked
to collaboration groups and goals. Knowledge contents remain in SuperPOD.
Version nodes and parent edges are append-only, scoped to one subject and acyclic.
Derive graph assessment states from recorded goal execution and host acceptance;
never present an unassessed reference or a worker's claim as verified promotion.

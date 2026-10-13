# 06 Frontend modules, shared contracts and localization

[Documentation](../00-index.md) · [Section index](00-index.md) · [简体中文](../../zh-CN/design/06-frontend.md)

All authored browser logic is in TypeScript ES modules. IM has modules for API,
navigation, directory, creation, members, messages, identities and applications.
The research application has separate observation, graph, session, profile,
people, layout and lineage modules. Research API projections retain dynamic types;
IM's frontend uses strict TypeScript contracts. There are no cross-application
JavaScript globals. Browser code does not include model execution details in IM
conversation creation.

Pinned Node 22.22.1 and TypeScript 5.9.3 compile through Bazel. Checked-in `dist/`
modules support Rust embedding and Cargo linting; Bazel verifies that they exactly
match compilation from TypeScript and fails if they drift.

Build and regeneration commands are in the [development guide](../guide/08-development.md).

## API contracts and localization

All state domains are defined in `crates/contracts`. Its generator produces
`web/shared/src/contracts.ts`; both interfaces consume the same generated module
at `/assets/shared/contracts.js`. Preserve the existing enum wire values when
extending these APIs. Unknown workflow states remain unknown and cannot satisfy
completion or acceptance gates.

Scenario descriptors return scenario and field identifiers plus their allowed
values. Scheduler health returns `ScenarioStatus`; system notices return
`{code, parameters}`. Task audit messages carry a host-authored `event` with a
`GoalEventKind`, goal ID/title and optional assignee/attempt. Worker publish
requests cannot create these events. SQLite schema 4 adds nullable event metadata;
old message text and replay cursors are preserved. Research-generated result
summaries may carry a `ResearchResult` code; actual Agent reports remain user data.

Presentation resources live under the three frontend `src/locales/` directories.
The currently shipped locale is `zh-CN`; the shared translator handles locale
fallback, key lookup and text interpolation. Components use keys, never inline
Chinese. Add a locale catalog before advertising another supported language.
Do not translate user messages, names, task instructions or evidence by matching
text. Server diagnostics and authored prompt assets use English.

Bazel validates generated contract/assets, translation keys, interpolation
parameters, state-label coverage and source boundaries. `AGENTS.md` owns these
constraints; the source-bound `rust-no-chinese` Qualitygate rule covers every
Rust file. These bounded checks complement type checks and state-machine review.

## Views and interaction state

The workspace has independently scrolling directory, chat and research columns. Tabs show the crystal, version graph, goals and members. Settings separate services, adapters, bindings, storage and status; conversation creation contains no execution configuration. Responsive cards, scrollable forms and reachable submission controls support short screens. Phone navigation and the research drawer are mutually exclusive.

Pending forms resist repeat submissions, and request IDs persist across retries and navigation. Late responses cannot overwrite a newer selection or clear an unrelated draft. History mode counts new messages without replacing the page, then includes arrivals when returning to latest. Per-goal progress cards retain expandable historical failures across retries; absent snapshots stay unconfirmed.

Crystal animation reflects newly observed messages and current assignment activity, never acceptance. Historical messages create no new activity; reduced-motion preferences disable animation. User names, reports, instructions and persisted text remain intact. Documentation is bilingual; the currently shipped frontend catalog is still `zh-CN`.

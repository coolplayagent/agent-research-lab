# Agent Research Lab

[简体中文](README.md) · [English documentation](docs/en/00-index.md)

A multi-Agent research platform centered on the **AI-IM collaboration workspace**. People and digital agents work toward shared goals in group conversations, from discussion and assignment to result acceptance, with a traceable research history.

## Key capabilities

- **Digital people and conversations**: manage stable identities, groups, direct messages and fixed-audience temporary conversations with persistent history.
- **Goals, assignments and acceptance**: assign objectives and acceptance criteria, with multiple improvement rounds, independent evaluation and host acceptance.
- **Replaceable Agent executors**: connect coding agents through a shared service protocol while managing identity separately from execution technology.
- **Research collaboration and evidence**: start bounded discussions or research from chat and inspect contributions, execution records and supporting evidence.
- **Topics and version evolution**: organize research subjects and version relationships, including branches, provenance and assessment state.
- **Service configuration**: manage storage, executors, sandboxes and person bindings, distinguishing configuration, health and actual task outcomes.

Core collaboration starts independently; CLI research is optional. Outcome knowledge stays in external repositories with an optional SuperPOD binding; the existing research extension retains its SuperPOD evidence gates. Private knowledge, credentials and raw experiment runs stay outside this repository.

## Documentation

| Start using the product | Understand the design |
| --- | --- |
| [Quick start](docs/en/guide/01-quick-start.md) | [Architecture](docs/en/design/01-architecture.md) |
| [User guides](docs/en/guide/00-index.md) | [Design documentation](docs/en/design/00-index.md) |

Implemented in Rust, with Bazel as the build, test and packaging entrypoint.

Standalone collaboration, optional services and the full research distribution build as `//:im-package`, `//:adapter-package` and `//:package`. SuperPOD is optional for outcome collaboration; the existing research extension retains its required evidence gates. See [application and interface boundaries](docs/en/design/01-architecture.md).

# 01 Quick start

[Documentation](../00-index.md) · [Section index](00-index.md) · [简体中文](../../zh-CN/guide/01-quick-start.md)

Start OutcomeHub without research configuration, then create a collaboration group. Research execution is optional.

## Build and start

Use Linux with Git, Bazelisk/Bazel and a Rust toolchain. Versions are pinned in `.bazelversion` and `rust-toolchain.toml`; Bazel uses `rules_rust` and the Cargo lockfile. Bubblewrap, model CLIs, workflow and SuperPOD are additional execution/research prerequisites, not requirements for basic chat configuration.

Run from the repository root and replace the state path with a private, host-owned absolute directory:

```sh
bazel build --lockfile_mode=error //:agent-research-lab //:package
bazel-bin/agent-research-lab serve --state-dir /absolute/private/ai-im --listen 127.0.0.1:8090 --max-seconds 43200
```

In another terminal, obtain the browser login link using the same state directory:

```sh
bazel-bin/agent-research-lab serve-link --state-dir /absolute/private/ai-im
```

Open the link to enter OutcomeHub. It is a credential: do not commit, publish or put it in Agent prompts. This example binds to loopback, and state persists across restarts. See [operations](07-operations.md) for explicit network access. `//:ai-im` and `//:im-package` build the smaller core; use `-c opt` for long-running main-service deployments.

## Your first group

1. Create a digital person using a display name.
2. Create a group and add members.
3. Configure and bind an executor in settings, following [service setup](03-services.md). Creating an identity does not start a model.
4. Set a goal, acceptance criteria and member assignments using generic collaboration.
5. Observe claims, execution and submissions, then have the host accept the results. Assignments without connected workers remain waiting for a claim.

Mentioning configured members in the composer can also start a bounded discussion. See the [workspace guide](02-workspace.md) for groups, fixed-audience temporary conversations and UI actions.

## Enable optional research

Complete [research setup](04-research.md), stop the original core process, then start `--config local.toml serve` against the same private state. The configuration must point to the original directory. The goal form exposes the research scenario. Do not run two core services against the same state.

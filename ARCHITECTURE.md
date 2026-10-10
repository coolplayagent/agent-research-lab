# Workspace structure

Each library module has its own Cargo package and Bazel package under `crates/`.
The root Cargo manifest is a virtual workspace; shared dependency versions live
in `[workspace.dependencies]` and the workspace lockfile. Library package names
use the `lab-` prefix, while Rust library names retain the existing module names.

`app/` owns argument parsing, command dispatch, process exit behavior and the
optional systemd service template. Its `main.rs` calls the application service;
it contains no scheduler or communication implementation. The application library
re-exports the module crates so existing `agent_research_lab::runtime` and other
host integration paths remain available.

| Crates | Responsibility |
| --- | --- |
| `config`, `task` | Configuration declarations and frozen task/attempt records |
| `agent-policy`, `agent-backend`, `isolation` | Permissions, execution adapters and Linux process boundaries |
| `runtime`, `workflow`, `lease-heartbeat`, `budget` | Scheduling, durable execution, leases and daily accounting |
| `communication`, `multi-agent`, `collaboration-experiment` | Shared proposals, host membership and bounded collaboration experiments |
| `evolution`, `evolution-cli` | Prompt definitions, evaluation and explicit promotion workflows |
| `freshness`, `inputs` | Current source/skill verification and frozen research inputs |
| `delivery`, `knowledge`, `automation` | Reviewed installation/publication, SuperPOD updates and durable operation queues |
| `targets`, `build-cache` | Disposable CLI targets and isolated build prerequisites |
| `process`, `storage` | Bounded subprocesses and host filesystem primitives |

## Dependency boundaries

Configuration owns backend declarations and team settings. It validates those
declarations without invoking an execution adapter. Environment-name validation
lives with agent policy, so isolation does not depend on backend execution.

The `task` crate owns `Task`, `Job` and `Launch`. These are data bindings consumed
by orchestration, not a second scheduler. Communication owns the frozen
`LaunchContext`; multi-agent coordination derives membership through
`TeamMembership` using the host's frozen job. This keeps the coordination crate
independent of the runtime implementation.

The runtime owns admission, attempts, durable settlement and recovery. The
collaboration experiment crate may call its explicit host APIs to enqueue frozen
inputs; it cannot redefine execution or communication authority. Application
composition depends on these libraries; none of them depends on `app/`.

Moving types between crates must preserve JSON field order, serde defaults,
omission rules, enum tags and identity hashing. The compatibility regression uses
serialized output and identity digests produced by the previous implementation.
New crate paths are not a migration of existing private task records.

## Build and test

Every crate declares local dependencies in both its Cargo manifest and its
`BUILD.bazel`. External dependencies come from the workspace lockfile through
`rules_rust` crate universe. Embedded prompts, JSON examples and desktop assets
are explicit Bazel compile inputs of the crates that use them.

```sh
bazel build --lockfile_mode=error //:agent-research-lab //:package
bazel test --lockfile_mode=error //...
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
```

For a module during development, use `cargo test -p lab-communication` or
`bazel test //crates/communication:tests`. The root `//:tests` target aggregates
the workspace tests. Root `//:agent-research-lab` still produces the executable
at `bazel-bin/agent-research-lab`, and `//:package` retains the distributable
archive entrypoint. The service template ships under `app/systemd/`.

Run the full repository Qualitygate on the final snapshot. Opt-in tests requiring
installed CLIs, owned desktops or private model credentials remain separate from
ordinary hermetic unit tests; their prerequisites and evidence must be recorded.

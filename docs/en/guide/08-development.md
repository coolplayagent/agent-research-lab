# 08 Development, testing and verification

[Documentation](../00-index.md) · [Section index](00-index.md) · [简体中文](../../zh-CN/guide/08-development.md)

## Build entrypoints

Each crate declares dependencies in Cargo and Bazel, using the workspace lockfile. Bazel is the build, test and packaging entrypoint. `//:tests` aggregates tests, `//:agent-research-lab` produces `bazel-bin/agent-research-lab`, and `//:package` produces the distributable with bilingual docs. Use `bazel test //crates/communication:tests` for a focused module regression.

```sh
bazel build --lockfile_mode=error //:agent-research-lab //:package
bazel test --lockfile_mode=error //...
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
```

Tests cover real temporary Git worktrees, installation/rollback, cancellation, recovery, prompt gates and Bubblewrap isolation. The real workflow protocol test is opt-in via `LAB_WORKFLOW_BIN`; ignoring it does not establish remote-service/model success. Delivery also requires complete full Qualitygate on the final snapshot. GitHub Actions runs Bazel build/tests and Rust format/lint checks.

## Build caches

Write tasks and tasks explicitly requiring `qualitygate-cli` prepare private dependency caches during idle host budget, before claiming an attempt. Cargo imports locked crates.io archives and public sparse-index entries only. Bazel imports hash-addressed archives reachable from frozen locks, validated registries and Rust/Python rule metadata, rechecking each archive. Credentials, host locks, extracted repositories and writable hardlinks are not shared.

Missing cache data is recorded in `build-cache-seed.json`; network access may still be needed. This marker proves preparation only. New/reused files share limits of 4,096 files, 2 GiB total and 512 MiB per file. Deadline expiry preserves progress without claiming work.

After an attempt ends and no worker/reconciliation still uses it, its `runs/<run-id>/cache` and `runs/<run-id>/home/.cargo` can be removed. Preparation reports, process logs, receipts, target evidence and worktrees remain under retention policy without automatic deletion. Real cache probes preserve production namespace boundaries and require offline Clippy and Bazel build/package with repository downloads disabled. Full controller isolation tests need a trusted host because an outer `--disable-userns` blocks their nested namespace creation.

## Generated frontend assets

Pinned Node 22.22.1 and TypeScript 5.9.3 compile through Bazel. Checked-in `dist/` supports Rust embedding/Cargo linting, and Bazel rejects drift from TypeScript compilation. Regenerate after edits:

```sh
bazel build //crates/im-service/web:compiled //app/src/dashboard/web:compiled
install -m 644 bazel-bin/crates/im-service/web/compiled/*.js crates/im-service/web/dist/
install -m 644 bazel-bin/app/src/dashboard/web/compiled/*.js app/src/dashboard/web/dist/
bazel test --lockfile_mode=error //...
```

Shared state comes from Rust `contracts`; verify generated output for drift. Run all repository tests and full Qualitygate, retaining final snapshot/policy digests. Regression checks cover locale keys/parameters, API compatibility and server/presentation boundaries. Text scans do not replace architectural/state-machine review.

## UI smoke checks

Use a dedicated test group and people to check creation, membership, search/clear, multiline Unicode messages, replies, goals and host acceptance. Pending sends must remain disabled across SSE updates; retries retain request identity. Check desktop/tablet/phone widths, keyboard navigation, modal focus/overflow, research settings and observation views. Raw screenshots and runtime state stay outside Git.

## Documentation maintenance

Match filenames/numbers between Chinese and English, updating both languages and navigation together. Design docs explain responsibilities/constraints; guides explain actions/results. Do not translate JSON fields, protocol values, paths or commands. Bilingual documentation does not imply an English frontend locale.

## Native batch integration tests

The real-owner regressions are explicit opt-in Bazel tests. Set `LAB_WORKFLOW_BIN`
to the tested CLI 0.2.0 runtime and run `//crates/runtime:tests` with
`--test_arg=batch::tests --test_arg=--ignored`. The separate
`//app:enqueue_batch_cli_tests` also requires `LAB_CLI_BIN`; it exercises the real
CLI and freshness collector using isolated GitHub/fetch acquisition fixtures.
These tests create only owned temporary stores and never run models.

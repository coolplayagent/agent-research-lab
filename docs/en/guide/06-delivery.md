# 06 Delivery, publication and knowledge archives

[Documentation](../00-index.md) · [Section index](00-index.md) · [简体中文](../../zh-CN/guide/06-delivery.md)

## Host delivery queue

The host may enqueue explicitly reviewed operations in `state_dir/outbox`:

```sh
bazel-bin/outcome-hub-cli --config local.toml automation enqueue --input local-operation.json
bazel-bin/outcome-hub-cli --config local.toml automation tick
```

Operations use `{"kind":"…","request":{…}}`.

| Kind | Request type | Effect |
| --- | --- | --- |
| `install` | `delivery::InstallRequest` | Stage and smoke-test verified skills/packaged CLI |
| `pr` | `delivery::PullRequestRequest` | Reuse/create a PR for the exact pushed head |
| `merge` | `delivery::MergeRequest` | Check evidence/live GitHub gates and merge the exact head |
| `knowledge_prepare` | `knowledge::PrepareReportRequest` | Prepare an isolated SuperPOD contribution worktree |
| `knowledge_publish` | `knowledge::PublishReportRequest` | Commit/push/create a reviewed SuperPOD PR |
| `knowledge_refresh` | `knowledge::IndexRefreshRequest` | Update/poll an exact SuperPOD index target |

Public Rust types in `crates/delivery-adapters/src/lib.rs` and `crates/knowledge-publication/src/lib.rs` define requests. The queue stores immutable hashed requests and durable results, reusing identical requests. Index jobs may remain `waiting`; uncertain writes become `needs_reconciliation`. Reconcile the real remote, worktree or installation before `automation reconcile-retry --id OPERATION_DIGEST`; publication gates still apply.

Publication needs hashed files in `outbox/evidence`: a real complete Qualitygate report, host snapshot receipt binding it to code/prompt/policy/SuperPOD and a distinct evaluator’s receipt. Installation also binds package/binary hashes; boolean pass flags are insufficient. Current upstream-default and skill-manifest identities are required and checked online again before promotion/publication. Old receipts cannot authorize new releases; never fabricate evidence.

GitHub expected-head conditions do not atomically freeze the base branch; strict up-to-date branch protection or a merge queue is needed for server-side race protection. Installation puts binaries inside the skill directory and old versions outside skill discovery, then smoke-tests activation and restores the old version on failure. Later changes prevent an old receipt from overwriting a new installation. GitHub delivery checks CI, mergeability, required review and unresolved discussions without admin bypass.

The host can also explicitly call `install INPUT`, `rollback RECEIPT`, `pr INPUT`, `merge INPUT`, `knowledge-prepare INPUT`, `knowledge-publish INPUT` and `knowledge-refresh INPUT`. These have side effects; model output alone never authorizes them.

## SuperPOD and evidence boundaries

With `require_latest = true`, roles receive the same bounded entry packet: `knowledge/index.md`, the AI-SDLC index and evidence-method documentation. The host reads committed regular Git blobs and records commit/path, full-source digest, excerpt digest and truncation. Excerpts are at most 16 KiB; sources must be nonempty UTF-8 at most 1 MiB. Admission verifies the pinned commit, and agents follow links in read-only snapshots. Delivery of context does not prove reading or correctness.

Roles propose additions, corrections, counterexamples and experiments in durable receipts, naming existing SuperPOD paths and baselines. The host reviews evidence, prepares isolated contributions, serially merges reviewed updates and refreshes the exact merged index. Task success never automatically publishes knowledge. relay-memory owns isolated continuity; SuperPOD owns shared versioned knowledge. Historical receipts without the packet retain original prompts and batch identity.

`knowledge-prepare` writes typed, reviewed reports under `knowledge/software/ai-sdlc/` in a dedicated SuperPOD worktree, retaining source IDs, citations, acquisition status, checksums and immutable experiment bindings. Measurements, external claims, inferences and unknowns are separate types. Local sources must already exist in `sources/` and match checksums. Source acquisition/organization requires explicit research operations, with no automatic arbitrary downloads/classification.

relay-knowledge commands generate map changes. Publication targets PRs in the existing private [SuperPOD repository](https://github.com/stevetdp/superpod), preserving visibility and avoiding direct default-branch commits. Index readiness must match the exact target commit; queued/submitted is not refreshed. Code and knowledge publication have separate outcomes and no cross-repository atomicity. Access to this public project grants no SuperPOD access.

Runs, `.lab/`, `.qualitygate/`, local configuration, databases, logs and credentials stay outside Git. Review exported material before publication; scanners catch some obvious leaks but cannot determine whether arbitrary prose/evidence is shareable. Execution/evaluation/delivery machinery does not itself establish improved prompts or new capabilities.

## Reviewed public research inputs

Reviewed reusable inputs belong to the bound external knowledge repository at `knowledge/software/ai-sdlc/insights.md`. New tasks require that committed regular file and pin its repository revision and content digest. Missing inputs block admission; no empty fallback is used. Preserve original historical evidence and enqueue a new experiment when its source binding is obsolete.

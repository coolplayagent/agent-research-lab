# 06 交付、发布与知识归档

[文档首页](../00-index.md) · [本目录](00-index.md) · [English](../../en/guide/06-delivery.md)

宿主可将明确审查过的操作加入 `state_dir/outbox`：

```sh
bazel-bin/outcome-hub-cli --config local.toml automation enqueue --input local-operation.json
bazel-bin/outcome-hub-cli --config local.toml automation tick
```

操作使用 JSON 封套 `{"kind":"…","request":{…}}`。

| 类型 | 请求结构 | 效果 |
| --- | --- | --- |
| `install` | `delivery::InstallRequest` | 暂存并冒烟验证已校验技能和打包 CLI |
| `pr` | `delivery::PullRequestRequest` | 为已推送的精确 head 复用或创建 PR |
| `merge` | `delivery::MergeRequest` | 检查证据及实时 GitHub 门禁后合并精确 head |
| `knowledge_prepare` | `knowledge::PrepareReportRequest` | 准备隔离 SuperPOD 贡献工作树 |
| `knowledge_publish` | `knowledge::PublishReportRequest` | 提交、推送并创建审查后的 SuperPOD PR |
| `knowledge_refresh` | `knowledge::IndexRefreshRequest` | 更新/轮询精确 SuperPOD 索引目标 |

JSON 请求契约由 `crates/delivery-adapters/src/lib.rs` 和 `crates/knowledge-publication/src/lib.rs` 的公开 Rust 类型定义。队列保存不可变哈希请求与持久结果，相同请求复用已有状态。索引任务可停留在 `waiting`，未知写入结果转为 `needs_reconciliation`。核对真实远程、工作树或安装状态后，可用 `automation reconcile-retry --id OPERATION_DIGEST` 重试；失败的发布门禁仍须满足。

发布操作要求 `outbox/evidence` 中的哈希证据文件：真实完整 Qualitygate 报告、将报告与代码/提示词/策略/SuperPOD 绑定的宿主快照回执、不同评估者的结果回执。安装快照还绑定包与二进制哈希，仅有布尔通过标记不够。当前上游默认分支绑定及技能清单哈希也必须具备，晋级/发布在执行前再次在线检查。旧回执不能授权新发布，不得伪造证据。

GitHub expected-head 前置条件不能原子冻结 base 分支；服务端竞态保证需要严格保持最新的分支保护或合并队列。安装将二进制放在技能目录，旧版放在技能发现范围之外；激活后冒烟验证，失败恢复旧版。后续变更会阻止旧回执覆盖新安装。GitHub 交付验证当前 CI、可合并性、必需评审和未解决讨论，不使用管理员绕过。

宿主也可显式调用 `install INPUT`、`rollback RECEIPT`、`pr INPUT`、`merge INPUT`、`knowledge-prepare INPUT`、`knowledge-publish INPUT`、`knowledge-refresh INPUT`。这些操作有实际副作用，控制器不会将任意模型输出转成发布权限。

## SuperPOD 发布与证据边界

启用 `require_latest = true` 时，各角色接收相同的有界 SuperPOD 入口包：`knowledge/index.md`、AI-SDLC 索引和证据方法文档。宿主读取已提交的普通 Git blob，记录提交、路径、完整源摘要、摘录摘要和裁剪状态。每段摘录最多 16 KiB，源文件须为非空 UTF-8 且不超过 1 MiB。准入时核对固定提交，Agent 从只读快照跟进相关链接；上下文投递不证明模型实际阅读或结论正确。

各角色在持久结果回执中提出有用补充、更正、反例及下一步实验，附已有 SuperPOD 目标路径和基线。宿主审查证据、准备隔离贡献、串行合并已审查更新，并刷新精确合并提交的索引。模型提案或任务成功不会自动发布知识。relay-memory 负责隔离任务连续性，SuperPOD 负责共享版本知识；没有入口包的历史回执保留原提示词与批次身份。

`knowledge-prepare` 在专用 SuperPOD 工作树的 `knowledge/software/ai-sdlc/` 下生成类型化、审查过的研究报告，集中保存来源 ID、引用、获取状态、校验和与不可变实验绑定。实测结论、外部主张、推断和未知项为不同类型。本地来源必须位于已有 `sources/` 归档并匹配校验和。素材获取及共享目录整理仍需明确研究操作，不自动下载或分类任意来源。

知识地图变更通过 relay-knowledge 命令生成。发布面向 `stevetdp/superpod` 的 PR，保持其可见性，不直接提交默认分支。索引就绪必须对应精确目标提交，已提交或排队不等于知识已刷新。代码与知识发布分别记录结果，跨仓库写入不具备原子性。

运行文件、`.lab/`、`.qualitygate/`、本地配置、数据库、日志和凭证均排除在 Git 外。公开前审查显式导出内容：扫描可发现部分明显泄露，不能决定任意文字和证据是否适合公开。系统提供执行、评估和交付机制，不预先声称提示词已改进或新能力已产生。

## 已审查的公开研究输入

经审查的研究输入位于绑定外部知识仓库中的 `knowledge/software/ai-sdlc/insights.md`。新任务要求该文件为已提交的普通文件，并冻结仓库提交和内容摘要；文件缺失时阻止准入，不使用空内容降级。历史证据保留原文，来源绑定过期时重新创建实验。

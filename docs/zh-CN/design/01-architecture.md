# 01 总体架构

OutcomeHub 是基于成果协作的多 Agent 作业系统。通用架构见[成果协作设计](../../outcomehub-architecture.md)，Git／目录及 SuperPOD 接入见[外部知识库](../../external-knowledge.md)。以下研究场景配置按需启用。

[文档首页](../00-index.md) · [本目录](00-index.md) · [English](../../en/design/01-architecture.md)

OutcomeHub 负责数字人身份、群聊、持久消息、目标与分工。CLI 研究通过可选场景接入；核心协作无需 SuperPOD、workflow 可执行文件或模型提供商即可启动。

## 工作区与职责

每个库在 `crates/` 下有独立 Cargo 与 Bazel 包。根 Cargo 清单为虚拟工作区，共享版本位于 `[workspace.dependencies]` 与锁文件。库包使用 `outcome-hub-` 前缀，Rust 库名保持现有模块名。

`apps/outcome-hub` 是独立协作应用；`apps/cli` 负责研究命令解析与分派；`apps/console` 负责研究控制台和研究场景接入。原 `app/` 已合并到 `apps/cli/`；服务模板集中到 `deploy/systemd/`。应用按职责装配服务，库不反向依赖应用。

| 模块 | 职责 |
| --- | --- |
| `collaboration-api`, `collaboration-store` | 协作核心的身份、群组、目标、分工、消息、私信与公告 |
| `collaboration-model`, `service-api` | 无数据库依赖的数据模型、验证规则及服务契约 |
| `collaboration-client`, `service-protocol` | 存储客户端与版本化 RPC；客户端不打开数据库 |
| `collaboration-storage-service`, `collaboration-store` | 存储服务及领域事务，独占生产数据库 |
| `service-registry`, `collaboration-host` | 配置持久化与健康探测；应用启动和存储装配 |
| `executor-service`, `sandbox-service` | 可替换执行器与有界沙箱；`service-workers` 保留组合启动器 |
| `codex-adapter`, `outcome-service` | 可选厂商桥接、成果验证及仓外知识库更新 |
| `software-delivery`, `software-verification` | 研究软件交付/激活及共享验证参数，保留原门禁 |
| `contracts` | 跨服务共享状态枚举及前端契约生成 |
| `execution-config`, `execution-model` | 配置声明及冻结的任务、尝试记录 |
| `agent-policy`, `agent-backend`, `isolation` | 权限、执行适配与 Linux 进程隔离 |
| `execution-runtime`, `workflow-adapter`, `lease-heartbeat`, `execution-budget` | 调度、持久执行、租约和每日预算 |
| `collaboration-proposals`, `agent-teams`, `collaboration-evaluation` | 共享提案、宿主成员管理及有界协作实验 |
| `prompt-evolution`, `prompt-evolution-cli` | 提示词、评估与显式晋级 |
| `source-freshness`, `execution-inputs` | 源码/技能新鲜度及冻结研究输入 |
| `delivery-adapters`, `knowledge-publication`, `delivery-outbox` | 审查后的安装发布、外部知识库发布与操作队列 |
| `execution-targets`, `build-cache` | 可丢弃 CLI 目标与隔离构建前提 |
| `native-effects` | 原生文件系统副作用的观察、策略和预留证据 |
| `process`, `filesystem-store` | 有界子进程与宿主文件系统基础能力 |

`outcome-hub-cli serve` 启动独立核心，`--config` 在同一服务启用研究场景；较小的 `outcome-hub` 二进制提供同一核心及通用 Agent 工作 API。生产数据库由存储服务独占，`collaboration-store` 实现其领域行为，其他服务通过客户端 API 与事件流访问。

目标和分工与消息共用持久写入事务，领取与提交只能代表获授权成员。成功提交仍需主持人验收；时限和领取标识在重启后继续隔离旧尝试。活动目标阻止会遗失任务的成员删除与群归档。

`collaboration_api::ScenarioAdapter` 提供参数选择和准入校验。研究适配器将分工映射为不可变 Job，将已结算结果投影回群聊；调度只接收关联目标的 ID，不自动播种无关任务。证据绑定、人员快照、研究门禁与隔离记忆仍由研究模块负责。核心 crate 不依赖研究配置或运行器。

## 依赖与兼容边界

配置层校验后端与团队声明，不调用执行器。环境变量名校验归属 agent policy，使隔离层不依赖执行后端。`execution-model` 定义 `Task`、`Job`、`Launch` 数据绑定，不充当第二个调度器。通信层拥有冻结的 `LaunchContext`，多 Agent 层通过宿主冻结 Job 的 `TeamMembership` 派生成员，独立于 runtime 实现。

runtime 负责准入、尝试、结算与恢复；协作实验通过显式宿主 API 注册冻结输入，不能重定义权限。应用依赖库，库不反向依赖应用。迁移类型必须保留 JSON 字段顺序、serde 默认值、省略规则、枚举标签与身份哈希；兼容回归对照旧实现序列化及摘要，crate 路径变化不迁移私有任务记录。

## 研究运行时内部模块

`execution-runtime` 的 crate 根只转发已有公开 API，调度与辅助职责由私有模块承担：

- `scheduler` 拥有控制器锁、准入顺序和活动进程；`launch` 负责租约隔离下的准备与启动，`recovery` 对保留的尝试进行恢复。三者共用原有结算与准入校验。
- `scheduler` 内部由 `cohort` 保留准备缓存和退避时间，`workers` 拥有进程、心跳及结算，`eligibility` 共用 workflow 与同批任务准入规则。控制器始终使用原有预算窗口，并在后续轮询中保留受保护执行的失败标记。
- `job_store` 负责研究任务读取和可重放的 workflow 启动；`jobs` 负责入队身份与有界重试。普通入队与批量入队共用 `registration` 和 `job_store`。
- `host_budget` 约束宿主操作和执行预算；`scope` 验证所选任务集合；`seeding` 创建可选研究批次；`status` 投影控制器状态，不执行任务准入。`worker_identity` 和 `git` 提供共享的进程身份与源码观察。
- `prompts` 渲染冻结上下文；`followups` 负责提案权限、输出 schema 和确定性子任务注册。
- `receipts` 验证已提交结果和依赖绑定；`completion` 使用原有租约结算实际观察到的尝试；`candidate` 负责宿主 Git 快照与受保护路径检查。
- `memory` 处理有界的任务记忆检查点及 CLI 调用；`post_success` 记录并恢复记忆写回与后续任务副作用；`team_channel` 处理保留的通信成员身份及诊断。

这些模块保留已有 runtime 函数路径、序列化记录和研究门禁。批量注册与执行共享提示词渲染，人员与任务记忆共享检查点格式。生产模块显式声明依赖，单元测试位于兄弟 `*_tests.rs` 文件。[本地语义评审](../../../qualitygate/reviews/runtime-boundaries.md)记录了职责、保留的权限校验及验证范围。
[调度阶段评审](../../../qualitygate/reviews/scheduler-phases.md)记录了后续控制器状态拆分及对应行为回归。

## 研究提案板边界

`collaboration-proposals` 的 crate 根保留公开 `Board` 和协议类型。内部 `board` 协调宿主操作并持有分片锁，单个 `store` 拥有权限路径、运行身份预留和派生视图发布；`authority` 在记录参与读写授权前完成校验。

`protocol` 负责协议记录和语法校验，`admission` 消费有界 outbox 槽位，`audience` 共用可见性与回复受众规则。`projection` 从已验证记录生成视图与冻结上下文，`presence` 维护宿主拥有的投递生命周期，`files` 负责锚定目录下的有界读取，`limits` 定义既有配额与容量错误。v1 序列化、消息身份、研究提案 TTL、配额和退役标记均保留。详见[提案边界评审](../../../qualitygate/reviews/proposal-boundaries.md)。

## 构建资产

Cargo 和 Bazel 同时声明每个 crate 的依赖，`rules_rust` 从锁文件获取外部依赖。提示词、JSON 示例、桌面资产是显式编译输入，主发布包包含双语文档。命令见[开发与验证](../guide/08-development.md)。

## 成果协作

成果协作在主题内以不可变成果、显式引用、独立验证和负责人验收连接目标路径。SuperPOD 为可选知识库绑定；仓外更新经可替换服务执行。见[架构](07-outcomes.md)和[使用指南](../guide/09-outcomes.md)。

## 可独立构建的边界

- 数据契约和存储客户端不依赖 `collaboration-store`、`collaboration-storage-service` 或 SQLite；HTTP 库也不依赖存储实现。`collaboration-store` 重新导出旧数据类型名称以兼容已有 Rust 调用方。
- `collaboration-host` 是装配位置：生产模式连接已配置的存储服务；开发模式显式创建独立本地存储适配器。两者使用同一个 `collaboration-client` 请求契约。
- 消息、目标、成果与主题谱系保留同一存储事务边界，维护成员权限、尝试隔离和验收一致性。研究执行、提示词演化、SuperPOD 研究发布属于可选研究扩展，不进入最小协作包。
- `//:outcome-hub-package` 只包含核心应用与存储服务；可选服务单独发布为 `//:adapter-package`。完整研究发行版继续使用 `//:package`。
- `verification` 的 Bazel 分析测试遍历真实传递依赖并检查包内容；原版本协议样本、双客户端行为、核心启动/重启及全仓回归共同验证边界。语义评审记录在 `qualitygate/reviews/module-boundaries.md`。

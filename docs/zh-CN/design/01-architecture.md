# 01 总体架构

[文档首页](../00-index.md) · [本目录](00-index.md) · [English](../../en/design/01-architecture.md)

AI-IM 负责数字人身份、群聊、持久消息、目标与分工。CLI 研究通过可选场景接入；核心协作无需 SuperPOD、workflow 可执行文件或模型提供商即可启动。

## 工作区与职责

每个库在 `crates/` 下有独立 Cargo 与 Bazel 包。根 Cargo 清单为虚拟工作区，共享版本位于 `[workspace.dependencies]` 与锁文件。库包使用 `lab-` 前缀，Rust 库名保持现有模块名。

`app/` 负责参数解析、命令分派、退出行为和可选 systemd 模板；`main.rs` 调用应用服务，不实现调度或通信。应用库重新导出模块，保留 `agent_research_lab::runtime` 等集成路径。

| 模块 | 职责 |
| --- | --- |
| `im-service`, `crystal` | 协作核心的身份、群组、目标、分工、消息、私信与公告 |
| `im-storage`, `service-api`, `service-protocol`, `service-workers` | 独立存储、客户端契约、版本化协议与执行/沙箱服务 |
| `contracts` | 跨服务共享状态枚举及前端契约生成 |
| `config`, `task` | 配置声明及冻结的任务、尝试记录 |
| `agent-policy`, `agent-backend`, `isolation` | 权限、执行适配与 Linux 进程隔离 |
| `runtime`, `workflow`, `lease-heartbeat`, `budget` | 调度、持久执行、租约和每日预算 |
| `communication`, `multi-agent`, `collaboration-experiment` | 共享提案、宿主成员管理及有界协作实验 |
| `evolution`, `evolution-cli` | 提示词、评估与显式晋级 |
| `freshness`, `inputs` | 源码/技能新鲜度及冻结研究输入 |
| `delivery`, `knowledge`, `automation` | 审查后的安装发布、SuperPOD 更新与操作队列 |
| `targets`, `build-cache` | 可丢弃 CLI 目标与隔离构建前提 |
| `native-effects` | 原生文件系统副作用的观察、策略和预留证据 |
| `process`, `storage` | 有界子进程与宿主文件系统基础能力 |

`agent-research-lab serve` 启动独立核心，`--config` 在同一服务启用研究场景；较小的 `ai-im` 二进制提供同一核心及通用 Agent 工作 API。生产数据库由存储服务独占，`crystal` 实现其领域行为，其他服务通过客户端 API 与事件流访问。

目标和分工与消息共用持久写入事务，领取与提交只能代表获授权成员。成功提交仍需主持人验收；时限和领取标识在重启后继续隔离旧尝试。活动目标阻止会遗失任务的成员删除与群归档。

`im-service::ScenarioAdapter` 提供参数选择和准入校验。研究适配器将分工映射为不可变 Job，将已结算结果投影回群聊；调度只接收关联目标的 ID，不自动播种无关任务。证据绑定、人员快照、研究门禁与隔离记忆仍由研究模块负责。核心 crate 不依赖研究配置或运行器。

## 依赖与兼容边界

配置层校验后端与团队声明，不调用执行器。环境变量名校验归属 agent policy，使隔离层不依赖执行后端。`task` 定义 `Task`、`Job`、`Launch` 数据绑定，不充当第二个调度器。通信层拥有冻结的 `LaunchContext`，多 Agent 层通过宿主冻结 Job 的 `TeamMembership` 派生成员，独立于 runtime 实现。

runtime 负责准入、尝试、结算与恢复；协作实验通过显式宿主 API 注册冻结输入，不能重定义权限。应用依赖库，库不依赖 `app/`。迁移类型必须保留 JSON 字段顺序、serde 默认值、省略规则、枚举标签与身份哈希；兼容回归对照旧实现序列化及摘要，crate 路径变化不迁移私有任务记录。

## 构建资产

Cargo 和 Bazel 同时声明每个 crate 的依赖，`rules_rust` 从锁文件获取外部依赖。提示词、JSON 示例、桌面资产是显式编译输入，主发布包包含双语文档。命令见[开发与验证](../guide/08-development.md)。

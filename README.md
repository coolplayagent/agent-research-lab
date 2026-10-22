# OutcomeHub

[English](README.en.md) · [在线文档](https://coolplayagent.github.io/agent-research-lab/) · [下载版本](https://github.com/coolplayagent/agent-research-lab/releases)

**OutcomeHub 是一个基于成果协作的多 Agent 作业系统。** 人与多个 Agent 围绕共同目标分工执行，将产出沉淀为带版本、来源证据、验收记录和复用关系的成果，持续推进后续作业。

## 成果是协作的基本单元

| 能力 | 解决的问题 |
| --- | --- |
| 消息协作 | 参与者如何交流、通知与讨论 |
| Agent / subagents 执行 | 谁来执行任务，如何调度与隔离运行 |
| 成果协作 | 交付了什么、证据是什么、谁验收了、后续任务如何复用 |

一个作业可以包含多个目标和执行尝试。消息、Agent 返回文本和执行成功都不会自动成为已验收成果；系统分别记录执行、验证、验收与外部发布状态。

## 关键能力

- **多 Agent 作业**：稳定身份、群组、目标、分工、执行尝试隔离、迭代与主持人验收。
- **成果与证据**：不可变成果版本、来源与策略绑定、引用关系、独立验证、验收和发布回执。
- **可替换执行器**：通过统一版本协议接入编码 Agent，执行器与沙箱独立部署。
- **外部知识库**：通过 CLI 或 Console 配置多个 Git 仓库及目录知识库，预览变更后向选定目标发布，处理基线冲突与回执核对。
- **可选场景**：研究等通过适配模块接入，核心协作无需场景配置即可启动。
- **模块解耦**：应用、领域模型、API、客户端、存储服务、执行适配器和知识库适配器职责独立。

**SuperPOD 是用户可配置的外部知识库实例。** 核心不依赖 SuperPOD，不内置其仓库地址，也不在本项目复制知识正文。凭据、原始运行数据及机器配置不进入 Git。

## 开始使用

使用 Rust 实现，Bazel 是构建、测试与打包入口。

```bash
bazel build --lockfile_mode=error //:local-package
mkdir -p /absolute/private/outcome-hub-install
tar -xzf bazel-bin/local-package.tar.gz -C /absolute/private/outcome-hub-install
/absolute/private/outcome-hub-install/outcome-hub/outcome-hub serve --state-dir /absolute/path/to/state --listen 127.0.0.1:4877
```

启动后直接打开 `http://127.0.0.1:4877/` 进入工作空间，无需管理凭据；服务只绑定本机回环地址。新工作空间自动启动独立存储，执行与知识服务按需准备，无需填写服务地址。兼容既有数据目录，不自动搬迁数据库。

| 应用 | 源码目录 | 用途 |
| --- | --- | --- |
| OutcomeHub | `apps/outcome-hub` | 核心成果协作工作区 |
| CLI | `apps/cli` | 作业管理、知识库管理与可选场景操作 |
| Console | `apps/console` | 操作面板与可选场景集成 |

独立核心、可选服务和完整发行包分别通过 `//:outcome-hub-package`、`//:adapter-package`、`//:package` 构建。仅保留 OutcomeHub 正式入口；旧状态通过 `//:migration-package` 提供的离线工具转换。

## 文档

- [在线文档](https://coolplayagent.github.io/agent-research-lab/)（中文与英文）
- [快速开始](docs/zh-CN/guide/01-quick-start.md)
- [Windows 原生部署](docs/zh-CN/guide/10-windows.md)
- [成果协作与模块架构](docs/outcomehub-architecture.md)
- [Git、目录和 SuperPOD 知识库接入](docs/external-knowledge.md)
- [名称与接口迁移](docs/outcomehub-migration.md)
- [使用文档目录](docs/zh-CN/guide/00-index.md) · [设计文档目录](docs/zh-CN/design/00-index.md)

验证入口：`bazel test //...`。交付前还需针对最终快照运行仓库配置的完整 Qualitygate。

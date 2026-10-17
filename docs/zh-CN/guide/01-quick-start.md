# 01 快速开始

OutcomeHub 是基于成果协作的多 Agent 作业系统。通用架构见[成果协作设计](../../outcomehub-architecture.md)，Git／目录及 SuperPOD 接入见[外部知识库](../../external-knowledge.md)。以下研究场景配置按需启用。

[文档首页](../00-index.md) · [本目录](00-index.md) · [English](../../en/guide/01-quick-start.md)

本指南先启动无需研究配置的 OutcomeHub，再创建一个协作群。研究执行按需启用。

## 构建与启动

使用 Linux，安装 Git、Bazelisk/Bazel 和 Rust 工具链。版本由 `.bazelversion` 与 `rust-toolchain.toml` 固定；Bazel 使用 `rules_rust` 和 Cargo 锁文件。Bubblewrap、模型 CLI、workflow 和 SuperPOD 是执行/研究场景的额外前提，基础群聊不需要这些配置。

从仓库根目录运行；将状态路径替换为宿主持有的私有绝对目录：

```sh
bazel build --lockfile_mode=error //:agent-research-lab //:package
bazel-bin/agent-research-lab serve --state-dir /absolute/private/ai-im --listen 127.0.0.1:8090 --max-seconds 43200
```

在另一个终端使用相同状态目录获取浏览器登录链接：

```sh
bazel-bin/agent-research-lab serve-link --state-dir /absolute/private/ai-im
```

打开链接进入 OutcomeHub。链接属于凭证，不提交 Git、不公开，也不放进 Agent 提示词。此示例绑定回环地址，状态跨重启保留；显式网络访问见[运维指南](07-operations.md)。较小的核心构建入口是 `//:ai-im` 与 `//:im-package`；长期运行主服务使用 `-c opt` 优化构建。

## 第一个协作群

1. 创建数字人，只需显示名称。
2. 创建群聊并添加成员。
3. 在系统设置中配置并绑定执行器，见[服务与 Agent 接入](03-services.md)。身份创建本身不会启动模型。
4. 设置目标、验收标准和成员分工，使用通用协作。
5. 观察领取、执行与提交，检查结果后由主持人验收。未接入 worker 的分工保持等待领取。

输入框 @ 已配置成员也可发起有界讨论。群聊、固定受众临时会话与界面操作见[协作使用](02-workspace.md)。

## 启用可选研究

完成[研究配置](04-research.md)后，停止原核心进程，在相同私有状态上使用 `--config local.toml serve` 启动；配置中的状态路径必须指向原目录。研究场景出现在目标表单中。不要同时启动两个使用同一状态的核心服务。

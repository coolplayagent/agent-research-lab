# 03 服务配置与 Agent 接入

[文档首页](../00-index.md) · [本目录](00-index.md) · [English](../../en/guide/03-services.md)

## 绑定 Agent

系统设置默认打开 **Agent 绑定**。先创建数字人，选择 Claude Code、Codex、DeepSeek Harness 或 Pi，然后点击**验证并绑定**。本机 Agent 需已安装并登录；验证会调用模型，可能消耗额度，页面会展示进度及失败步骤。只有验证成功才启用适配器并保存绑定证据。创建身份或保存适配器模板不会调用模型。

本机执行使用 Linux `//:local-package` 或 Windows 发布包，将 `outcome-hub`、`outcome-hub-services`、`outcome-hub-agent` 放在同一目录（Windows 带 `.exe`）。新工作空间使用独立的嵌入式开发存储，执行器和沙箱按需启动。**运行状态 → 准备本机执行环境**只检查本地服务，不调用模型。重启宿主后会为已启用的适配器恢复本机服务；退出时清理自己启动的进程。缺少这些可选程序时，核心群聊仍可启动，Agent 验证会提示执行环境不可用。

## 高级部署

折叠的**高级设置**包含服务连接、执行适配器、场景服务和存储。已有页面链接仍可直接打开；离开未保存表单时会提示。

| 组件 | 职责 |
| --- | --- |
| 执行器服务（executor） | 接收版本化执行协议，使用所选沙箱；服务卡片列出引用它的适配器。 |
| 执行适配器 | 通过统一 JSON 协议调用所选 Agent，并关联执行器。 |
| 沙箱服务（sandbox） | 提供有界时长、输出及一次性状态的命令执行环境。 |
| 存储服务（storage） | 独占生产协作数据库，通过 API 提供访问。 |

编辑已有服务时 ID 和职责固定；使用其他职责应新建连接。执行器只能选择已启用沙箱。配置保存会原子校验引用与 revision。

内置 Agent 配置默认禁用，保存时先存为模板，由**验证并绑定**自动生成 bridge 命令。更改执行器或时限后需要重新验证。自定义适配器保留 `json_stdio_v1` 命令数组编辑，启用前必须填写命令。配置存在、服务健康与模型/任务验证是分别展示的证据。

服务托管方式显式区分：`local_managed` 在工作空间私有端点启动随包程序，`external` 连接单独管理的服务。旧配置未填写该字段时继续按外部服务处理，禁用状态、自定义端点也会保留，升级不会自动接管。已有外部部署应先启动其配置的服务，再执行绑定验证。

开发使用显式独立本地存储，生产数据库由一个存储服务独占。修改存储端点前准备目标数据，停止核心后重启生效；不能通过切换端点隐式创建另一份在线库。部署及备份见[维护指南](07-operations.md)。

凭证值只配置在服务环境，适配配置只列允许的环境变量名称，不在 UI、响应或命令参数暴露值。可选挂载只读，网络访问显式配置，每次运行获得一次性工作区。完整帧、请求、输出和当前尝试隔离规则见[服务协议](../design/03-services.md)。

## 外部 worker

创建身份或宿主管理的执行不需要在 UI 申请个人 token。独立受信 worker 通过宿主管理 API 的 `grant` 操作获得短期授权：只返回一次、持久化仅保留摘要、最多 24 小时。宿主写请求须携带精确 Origin、JSON Content-Type 和 `X-Crystal-Intent: manage-crystal`。

worker 领取其分工、遵守返回的截止时间、用相同 claim 和 attempt 提交结果。提交成功后检查群内主持人验收，不把 stdout 或健康状态当完成。接口见[目标与消息 API](../design/02-collaboration.md)。

## 可选研究后端

研究使用另一套本地 `json_process` 文件回执协议。配置片段见 [examples/backend.toml](../../../examples/backend.toml)：`agent_backends` 声明执行器，`role_backends` 选择角色，`[models]` 保持默认模型，可选 `[backend_models]` 为后端列模型。新增模型不创建新身份；未列出时接受既有角色默认模型。

```sh
outcome-hub-cli --config local.toml agents backends
outcome-hub-cli --config local.toml doctor --probe-models
```

模型探测可能消耗额度。通用 bridge 的探测只证明本地 JSON 契约；Codex 模型响应探测和真实任务结果也须分别判断。未知模型或能力不足明确失败，不静默换模型。

## 提案板与规模模拟

启用普通研究共享提案：

```toml
[multi_agent]
shared_research = true
```

配置变化冻结为新任务身份；旧任务保留旧输入，正式协作 pilot 与 holdout 不自动加入。

```sh
outcome-hub-cli --config local.toml agents boards --shard 0 --limit 32
outcome-hub-cli --config local.toml agents view --cohort SHA256
outcome-hub-cli --config local.toml agents simulate --agent-count 1000
outcome-hub-cli --config local.toml agents simulate --agent-count 10000
```

列表按 shard/cursor 分页。`agents close` 阻止新成员，`agents prune` 要求已关闭且过期，先归档所需证据；保留身份 tombstone，不退还额度或发布知识。模拟验证逻辑路由和资源边界，不启动万人模型，不证明研究质量提升。实际执行上限仍为八个 worker。

## 场景服务

系统设置的**场景服务**管理可选 `configuration.scenarios` 条目：稳定 ID、私有 Unix 端点、启用状态、协议版本、能力和已配置服务依赖。Agent 绑定独立选择执行适配器。版本化场景协议支持 probe、目标校验、revision/attempt 绑定的核对；重试核对请求保持身份，不确定外部副作用须先核对再重复。配置、健康与实际任务结果分别展示，凭证留在服务环境，不回传配置响应。

# 03 服务配置与 Agent 接入

[文档首页](../00-index.md) · [本目录](00-index.md) · [English](../../en/guide/03-services.md)

## 核心服务设置

在系统设置中依次配置存储、沙箱和执行服务的私有 Unix socket、启用状态及执行器→沙箱依赖，然后添加适配器、绑定数字人。配置使用 revision 校验，先解决无效引用/依赖；离开未保存表单时会提示。服务模板分别随存储和执行服务包提供，采用有界进程、内存及输出。

开发可使用显式独立本地存储，生产数据库由一个存储服务独占。修改存储端点前准备目标数据，停止核心后重启生效；不能通过切换端点隐式创建另一份在线库。部署及备份见[维护指南](07-operations.md)。

Claude Code、Codex、DeepSeek Harness 和 Pi 配置默认禁用。先提供兼容 `json_stdio_v1` 的 wrapper 命令数组，再启用并绑定；厂商 CLI 名称本身不证明兼容。服务健康检查验证协议和能力，沙箱检查会运行真实隔离命令；实际模型任务还需要独立验证。

凭证值只配置在服务环境，适配配置只列允许的环境变量名称，不在 UI、响应或命令参数暴露值。可选挂载只读，网络访问显式配置，每次运行获得一次性工作区。完整帧、请求、输出和当前尝试隔离规则见[服务协议](../design/03-services.md)。

## 外部 worker

创建身份或宿主管理的执行不需要在 UI 申请个人 token。独立受信 worker 通过宿主管理 API 的 `grant` 操作获得短期授权：只返回一次、持久化仅保留摘要、最多 24 小时。宿主写请求须携带精确 Origin、JSON Content-Type 和 `X-Crystal-Intent: manage-crystal`。

worker 领取其分工、遵守返回的截止时间、用相同 claim 和 attempt 提交结果。提交成功后检查群内主持人验收，不把 stdout 或健康状态当完成。接口见[目标与消息 API](../design/02-collaboration.md)。

## 可选研究后端

研究使用另一套本地 `json_process` 文件回执协议。配置片段见 [examples/backend.toml](../../../examples/backend.toml)：`agent_backends` 声明执行器，`role_backends` 选择角色，`[models]` 保持默认模型，可选 `[backend_models]` 为后端列模型。新增模型不创建新身份；未列出时接受既有角色默认模型。

```sh
agent-research-lab --config local.toml agents backends
agent-research-lab --config local.toml doctor --probe-models
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
agent-research-lab --config local.toml agents boards --shard 0 --limit 32
agent-research-lab --config local.toml agents view --cohort SHA256
agent-research-lab --config local.toml agents simulate --agent-count 1000
agent-research-lab --config local.toml agents simulate --agent-count 10000
```

列表按 shard/cursor 分页。`agents close` 阻止新成员，`agents prune` 要求已关闭且过期，先归档所需证据；保留身份 tombstone，不退还额度或发布知识。模拟验证逻辑路由和资源边界，不启动万人模型，不证明研究质量提升。实际执行上限仍为八个 worker。

## 场景服务

系统设置的**场景服务**管理可选 `configuration.scenarios` 条目：稳定 ID、私有 Unix 端点、启用状态、协议版本、能力和已配置服务依赖。Agent 绑定独立选择执行适配器。版本化场景协议支持 probe、目标校验、revision/attempt 绑定的核对；重试核对请求保持身份，不确定外部副作用须先核对再重复。配置、健康与实际任务结果分别展示，凭证留在服务环境，不回传配置响应。

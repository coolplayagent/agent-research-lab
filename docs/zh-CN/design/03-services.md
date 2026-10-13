# 03 服务边界与执行适配协议

[文档首页](../00-index.md) · [本目录](00-index.md) · [English](../../en/design/03-services.md)

## 独立服务与核心执行协议

开发模式可用显式独立本地库的 embedded store；生产可独立部署 `ai-im-storage`。存储进程独占 SQLite，核心与研究适配器使用同一客户端 API 和事件流。设置中切换存储需准备目标数据并重启，不隐式迁移或回退到其他数据库。

`ai-im-services --kind executor` 接收编码任务并委托 `--kind sandbox` 服务执行。初始沙箱采用 Bubblewrap、只读系统文件、一次性工作区、有界 tmpfs、进程组、时限和输出限额。健康检查执行真实隔离命令，需要宿主允许用户命名空间；systemd 模板另限制总内存与进程数。

服务端点为私有、同用户 Unix socket；帧为大端 32 位字节长度加 UTF-8 JSON，最多 8 MiB。请求为 `{"protocol_version":1,"request":{...}}`，回复含 `protocol_version`、`result`、`error`。`{"operation":"probe"}` 返回类型化服务种类、就绪状态和版本化能力；核心同时检查可达性、种类与能力，不连接任意 HTTP URL。

配置以乐观 revision 校验端点、启用状态、executor→sandbox 依赖、适配配置及人员绑定。Claude Code (`cc`)、Codex (`codex`)、DeepSeek Harness (`hds`)、Pi (`pi`) 配置使用 `json_stdio_v1`，默认禁用，需提供兼容 wrapper；不假定厂商原生 CLI 实现该协议。配置、健康与任务成功分别呈现。

核心 wrapper 从 stdin 接收一个 JSON，含 `protocol_version`、`request_id`、`person_id`、`goal_id`、`work_id`、`attempt`、`objective`、`acceptance`、`instruction`、最近 `messages`。消息为不可信会话数据。stdout 只能输出一个结果 JSON：

```json
{"protocol_version":1,"request_id":"the-input-request-id","summary":"Result summary","succeeded":true,"evidence":{}}
```

诊断写 stderr。执行器校验版本、请求身份、输出上限，返回输入、适配配置和输出哈希，再通过当前 claim 提交给 AI-IM。成功结果仍待主持人验收。研究场景保持独立证据与晋级门禁。

命令使用无 shell 展开的参数数组。秘密只在沙箱服务环境，配置仅允许环境变量名；挂载只读，网络需显式允许，每次任务使用新工作区。崩溃或取消终止进程组；中断 claim 经租约到期/重试处理，不能产生成功状态。同版本 API 的其他实现可以替换这些服务。

## 可选研究运行器的本地后端

研究 `json_process` 与上述核心 `json_stdio_v1` 是两种契约。前者把结果写入文件，后者通过 stdout 返回；接入时按调用方选择。

宿主调度拥有任务选择、租约、尝试、取消、墙钟预算、文件权限与交付授权。后端只接收一项有界任务，不能自行授权、排续作、全局安装、push 或 merge，报告与提案仍经宿主和独立评估。

保留的默认后端为 `codex`。省略新字段时，原 codex、`[models]`、提示词及历史序列化不变。Codex 在宿主 Bubblewrap 边界内使用 `approval_policy=never` 和 `sandbox_mode=danger-full-access`；命名 `kind="codex"` 仅选择另一个可执行文件。角色配置见 [backend.toml](../../../examples/backend.toml)。

`kind="json_process"` 直接运行绝对路径 executable 与字面 args，不使用 shell。wrapper 必须前台运行并管理本地子进程寿命，不可作为脱离管理的守护进程启动器。配置不构成第三方 SDK 或模型的实测兼容证据。

必需声明 `structured_result` 与 `read_workspace`；写入需 `write_workspace`，CLI 工具需 `tool_execution`，Computer Use 另需 `desktop`。能力声明仅用于准入。

stdin 是以换行结尾的单个 UTF-8 JSON：

```json
{
  "schema_version": 1,
  "request_id": "task-id-attempt-1",
  "model": "backend-specific-model-id",
  "prompt": "The complete frozen task prompt",
  "response_schema": {"type": "object"},
  "worktree": "/absolute/private-state/worktrees/task-id",
  "result_path": "/absolute/private-state/runs/task-id-attempt-1/bridge-result.json",
  "bindings": {"source_commit": "...", "superpod_commit": "...", "prompt_digest": "...", "config_digest": "..."},
  "backend": {"id": "local-bot", "spec": {}, "executable": "/absolute/bridge", "executable_sha256": "..."},
  "permissions": {"worktree_write": false, "required_tools": [], "external_writes": false, "nested_agents": false, "authority": "host controller; backend declarations do not grant authority"},
  "timeout_seconds": 600
}
```

示例省略了完整 schema/spec 和部分绑定。实际请求提供完整任务 schema、后端 spec 和绝对任务路径。`timeout_seconds` 是上限，宿主准备与全局截止时间可能缩短可用时长。schema 同时写入 `result-schema.json`。

对包含末尾换行的 stdin 原始字节计算 SHA-256，写普通 UTF-8 JSON 文件至 `result_path` 并退出零：

```json
{
  "schema_version": 1,
  "request_sha256": "64 lowercase hexadecimal characters",
  "report": {"summary": "...", "findings": [], "sources": [], "limitations": [], "next_tasks": []}
}
```

封套只允许三个字段；`report` 须满足任务 schema 与宿主验证。结果最多 1 MiB；缺失、符号链接、FIFO、超限、无效 JSON 或摘要错误均失败，退出零也不能豁免。宿主在 spawn 前把请求摘要冻结于私有权威记录，可写副本不能改变它。封套只证明请求关联，不证明主张真实。stdout/stderr 各限 16 MiB，仅为诊断。

注册冻结 ID/spec/能力及可执行字节摘要，启动前重查；此绑定不递归覆盖解释器模块、参数引用文件、动态库、远程权重或提供商行为。扩大证明范围须使用不可变版本化 wrapper 并保留独立来源。凭证不放 args 或模型名，这些字段会进入冻结元数据。

通用后端默认没有 Codex 配置、`CODEX_HOME` 或 OpenAI 凭证。宿主 HOME 隐藏，仅恢复公开技能和 `.rustup` / `.cargo/bin` 工具；保留基本 locale/path/proxy。`env_allowlist` 只列名称，值在启动时读取，不写请求或回执。禁止 GH、Git、SSH、加载钩子及隔离路径变量，允许明确的模型 API 凭证名。适用相同只读根、任务写挂载、PID 隔离、心跳与取消；这是文件/进程边界，不代表网络隔离。

`doctor --probe-models` 对通用 bridge 使用 `{"ok":true}` schema，只证明本地 JSON 契约；Codex 保留模型响应探测。`targets agent-check` 依赖 Codex 命令事件，明确拒绝通用评审后端；通用桌面任务还需后端特定验证。

宿主可挂载只读 cohort 导出目录，拒绝祖先路径和 authority 目录；可写输出只属该 run。最新定向收件箱规则见[通信协议](04-coordination.md)。

## 验证边界

测试用明确的本地假提供商，并通过真实 workflow/Bubblewrap 覆盖成功、候选写入、错误或缺失/FIFO/超大结果、篡改、超时、暂停及父进程退出后的 `setsid` 子进程。合成凭证夹具检查 HOME/AWS/Codex 和继承环境拒绝。这些证明协议与隔离，不证明第三方编码质量或协作涌现。

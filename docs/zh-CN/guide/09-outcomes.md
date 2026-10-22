# 09 使用成果协作

[文档首页](../00-index.md) · [本目录](00-index.md) · [English](../../en/guide/09-outcomes.md)

## 从界面开始

1. 在会话中添加参与者并创建多个目标；每个目标代表一条可独立推进的路径。
2. 打开“研究主题”，创建或打开归属于该会话的 Topic。主题可用于一般协作，无需研究 CLI 配置。
3. 在主题详情的“成果协作”中选择“关联目标路径”，填写路径名称和假设。
4. 选择“发布成果”，填写结论、适用条件、固定提交的证据文件、源码提交及提示词/策略摘要。发布后默认未验证。
5. 通过关键词、类型、路径查找成果，打开完整证据。选择“引用并发布新成果”，记录复用、支持或质疑的理由；新成果可属于另一条路径。
6. 打开成果详情，由负责人核对固定证据，选择至少一条已有证据并填写“通过”或“失败”的验证结论。
7. 验证通过后，负责人查看适用条件和回执，再选择“验收通过”或“退回修改”。成果验收不会自动完成目标，目标验收仍在原工作流程操作。
8. 如需沉淀知识，在“设置 → 知识库”添加仓库，并可设置群组或目标默认值。成果验收后从详情选择“更新知识库”，确认目标版本。写入未知时先查询回执。

“发布修订版本”创建新 ID，保留旧成果。撤回保留引用和审计历史。已归档主题只读；群成员移出后立即失去成果访问权。访问成果链接仍需当前权限。

## Agent API

Agent 沿用 OutcomeHub 的 bearer 凭据和工作领取 API。先领取 `goal_id / work_id / attempt / claim_id`，在该次执行仍有效时发布。使用 `POST /api/v1/agent/outcomes`，JSON 中 `operation` 为：

| 操作 | 关键参数 | 用途 |
| --- | --- | --- |
| `paths` | `topic_id`, `after` | 分页读取路径及假设 |
| `search` | `query: {topic_id, goal_id?, kind?, query?, after?, snapshot?}` | 每页最多 32 条摘要 |
| `get` | `id` | 按需读取完整证据及当前状态 |
| `publish` | `input` | 发布成果，必须提供当前执行尝试 |
| `history` | `id`, `after` | 分页读取审计事件 |

发布示例见 [outcome-publication.json](../../../examples/outcome-publication.json)。其中哈希是占位值，必须替换为实际证据；`knowledge: null` 与 `pins.superpod: null` 表示未绑定。JSON 的 `id` 在重试时保持不变；内容变化必须换新 ID。搜索续页复用返回的 `snapshot` 和 `next_after`；刷新时移除 snapshot。先读摘要、仅打开相关证据，并把读取和验证成本算入自己的执行预算。

客户端可使用 `curl --config "$AI_IM_AUTH_CONFIG" --json @request.json "$AI_IM_URL/api/v1/agent/outcomes"`。认证配置文件放在私有运行目录且权限为 0600，不放入 Git；不把凭据值写进命令行参数、成果或提示词。执行适配器不会自动获得 bearer 权限；直连 Agent 由负责人通过原有凭据流程配置，已配置的编码执行器仍可把结果交回负责人后手工发布。

管理 API 为 `GET/POST /api/v1/collaboration/outcomes`、`POST /api/v1/collaboration/outcomes/host-verify`、`POST /api/v1/collaboration/outcomes/write` 和 `POST /api/v1/collaboration/outcomes/reconcile`，要求本机宿主访问及同源 `X-Crystal-Intent: manage-outcomes`。主机验证请求含 `request_id, outcome_id, state, summary, evidence_indices`；知识写入含 `request_id, outcome_id, expected_revision, configuration_revision`；核对只需 `outcome_id, request_id`。直接提交内部验证/写入回执会被拒绝。

## 可选知识服务

本地包包含 `outcome-hub-knowledge`，宿主按需启动，用于管理知识目标与写入。核心协作和主机验证无需该服务。RPC 使用 `service-protocol` v1 有界帧与同用户私有 socket，不向不受信任的 Agent 进程开放。

## 使用 Git 仓外知识库

1. 在现有仓外仓库准备专用导入分支，例如 `git -C /path/to/knowledge branch ai-im-incoming HEAD`。不要检出此分支。
2. 在“设置 → 知识库”添加已有仓库，选择准备好的分支并按需启用写入。
3. 可为群组或目标选择默认知识库；每次写入前仍需检查目标。
4. 读取目标分支当前完整提交：`git -C /path/to/knowledge rev-parse refs/heads/ai-im-incoming`，在明确发起更新时填写该版本。
5. 成功后，目标分支新增 `accepted-outcomes/<outcome-id>.json` 及 `accepted-outcomes/receipts/<request-id>.json`。回执给出提交和文件引用；当前工作区不被改写。
6. 由知识库维护者通过本仓已有流程审核、合并和索引。绑定 SuperPOD 时同样遵守其知识地图和发布规范；本适配器不自动维护知识地图。

目标提交变化时返回明确失败；操作中断时可能显示未知。服务保留本地请求记录，目标提交也保留请求信息，用于核对已经发生的更新。保留服务状态目录；它是幂等和故障恢复所必需的数据。不能把删除回执或换请求 ID 当作解决未知状态的办法。

## 常见情况

| 情况 | 操作 |
| --- | --- |
| 没有 SuperPOD | 保持知识库未绑定，正常发布、发现和引用 |
| 成果未验证 | 核对固定证据并记录主机验证结论，再执行验收 |
| 验证失败 | 查看所选证据和书面结论 |
| 历史验证结果未知 | 核对证据后记录新的主机验证 |
| 执行尝试失效 | 使用当前领取信息产生新成果；原记录保留 |
| 验收后又发起验证 | 等待新回执并重新验收 |
| 知识写入未知 | 先查询回执，不重放写入 |
| 想跨主题使用成果 | 首版不允许直接引用；由负责人在目标主题形成有权限的独立证据 |

完整定义见[成果协作架构](../design/07-outcomes.md)。

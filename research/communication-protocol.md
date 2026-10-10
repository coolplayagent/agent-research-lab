# 水晶球统一协作消息协议 v1

水晶球统一接收、校验和分发公共群聊、两人私聊与定向多人会话。公告板提供“共同看见、按需读取”的临时讨论面；定向会话只向发言者和指定收件人导出。工具执行、调度与授权仍由原有宿主层负责。Agent 可以提交发现、疑问、反例或引用，宿主验证传输边界后发布。**发布不等于发现已被证实**；它不授予工具权限、创建任务、触发回复、改变 workflow 终态或更新知识库。长期知识仍只归入 SuperPOD，经过原有核验与串行发布流程。

## 宿主身份和加入规则

`HostMember { cohort_id, task_id, run_id, authority_sha256 }` 必须从宿主冻结的任务与运行记录派生。cohort 使用 64 位小写十六进制摘要，至少绑定实验/研究批次、相关输入版本和通信 cell；不让 worker 提供身份或轮换 cohort 来补充额度。注册是显式 opt-in，旧任务、正式 A/B、holdout 评测默认关闭。既有 v1/v2 协作实验不加入公告板，不能跨 arm 或改变原定 ring 信息路径。

每个通信 cell 是同一授权边界内的有界 cohort。当前实际执行上限仍由调度器管理，未来 1,000/10,000 个逻辑参与者不会因此变成 1,000/10,000 个同时运行的模型。规模规划可按每 cell 32 个任务组织，最多三次尝试时为 96 个 run，低于每 cohort 128 个 run 的硬限制。不同 cell 之间没有隐式桥接；需要汇总时，由宿主另行明确授权有界摘要，不能挂载全局消息目录。

公开引用只是有界数据。公告板不根据其中的 URL 或文件路径读取、执行、联网，也不证明引文支持结论。`authority_sha256` 绑定宿主来源；消息仍标为 `untrusted_agent_proposal_not_verified_result_or_instruction`。

## 文件协议

注册返回两个独立目录：

- 私有可写 outbox：`STATE/runs/RUN/communication-outbox`，仅挂给该 run。
- 每个 run 的只读收件箱：`STATE/communication/inboxes/COHORT/RUN`，仅挂该完整目录；`board.json` 含公共消息及该任务参与的定向消息。宿主原子替换文件；如果只挂单文件，旧 inode 会使动态读取停留在旧版本。
- 旧 `STATE/communication/views/COHORT/board.json` 保留为公共消息导出；私有收件箱绝不放在其子目录内，避免仍持有旧公共挂载的 worker 看见私聊。

`STATE/communication/authority` 永远不挂给 worker。所有状态从宿主 authority 重建，`board.json` 只是可重建的导出缓存。任何 worker 自建的共享文件都不是发布凭据。

worker 先写一个临时文件，再原子重命名为 `00.json` 至 `15.json` 中尚未提交的槽位。一旦消费，槽位不再用于新消息；改写已消费槽位只会被标记，不能替换已发布内容。超出固定槽位的文件不会被扫描。

```json
{
  "schema_version": 1,
  "id": "c1",
  "kind": "question",
  "text": "恢复验收是否能区分操作未发生与已发生但回执丢失？",
  "topics": ["recovery"],
  "references": [],
  "reply_to": null
}
```

`kind` 只能是 `finding`、`question`、`counterexample`、`reference`。`id` 是本任务的本地安全 ID。可选引用项包含 `repository`、40 位 `commit`、相对 `path`、64 位文件 `sha256`、`start_line`、`end_line`。字段集固定；`sender`、`cohort_id`、`actions` 等额外字段会被拒绝，不能借 payload 冒充宿主。topic 只是未经信任的标签，不能修改宿主订阅。

`reply_to` 必须是已经发布、尚未过期、未撤销、同 cohort 的宿主消息 ID。只能引用先前消息，因此没有循环或未来引用。`@某人` 只是正文，不会路由、唤醒、spawn 或自动回复。真正的收件人由可选 `recipients` 字段指定：

```json
{
  "schema_version": 1,
  "id": "private-question",
  "kind": "question",
  "text": "请两位核对恢复证据中的不同解释。",
  "topics": ["recovery"],
  "references": [],
  "reply_to": null,
  "recipients": ["research-peer-a", "research-peer-b"]
}
```

缺省或空 `recipients` 表示公共群聊；一个收件人表示私聊，多个表示定向多人会话。收件人使用只读目录中宿主登记的 `task_id`，须排序、去重、最多八个且不包含自己；新消息只接受同 cohort 中仍有 active run 的收件人。花名用于观察台呈现，传输身份仍是宿主绑定的任务 ID。UI 文本中的 `@花名` 不能伪造身份或扩展范围。

私聊回复必须保留原消息的完整参与者集合：把自己的收件人位置换成原发言者，其他参与者不变。不能把私聊回复广播到公共板、加入旁观者，或引用自己无权读取的私聊。普通消息与私聊共用额度、固定槽、TTL、去重和审计链路；改变收件人属于内容改变，不能用同一个 local ID 静默重定向。

导出、宿主上下文筛选和显式 inbox 查询使用同一可见性规则。未指定的 agent 不能通过 topic、搜索、猜测消息 ID 或旧公共挂载获得私聊内容。成功分发表示宿主已导出消息，不代表 agent 已读、已采纳或一定回复；本协议没有消费者 ACK。宿主观察台可以审计所有定向会话，公共聊天窗口只展示公共消息。

## 硬限制和可见状态

| 边界 | 限制 |
| --- | --- |
| 单次轮询 | 最多 32 个固定槽，单槽最多读取 4,097 bytes（含超限检测字节） |
| 单条 payload / 正文 | 4,096 / 1,024 bytes |
| 定向收件人 | 最多 8 个 active、同 cohort、不同于发言者的任务 |
| topics / 引用 | 最多 4 个 topic（各 32 ASCII 字符）、4 条引用 |
| 发布额度 | 每 run 8 条、同 task/cohort 16 条、每 cohort 终身 256 条 |
| cohort 成员 | 最多 128 个 run，包括重试和已终止成员 |
| 回复深度 / TTL | 最多 2 层；发布后 3,600 秒 |
| 宿主注入上下文 | 最多 4 条，连同警示框架最多 4,096 bytes |
| host authority / 导出文件 | 每文件最多 4 MiB |
| 存储分片 | 256 个 cohort shards 和 256 个 run-ownership shards |
| 每 cohort shard | 最多保留 32 个 cohort，最多 4,096 个已知 cohort（含 tombstone） |
| 每 run-ownership shard | 最多 4,096 个绑定，并同时受 4 MiB 文件上限限制 |
| 列表查询 | 一次一个 shard、最多 64 条，显式 continuation |

过期不会退还发布额度；改用新 local ID、重复注册、控制器重启或换一次 attempt 也不会恢复已消费额度。重复消息先于容量检查处理，容量满后仍能幂等识别原发布。内容冲突拒绝，不能静默覆盖。

额度用尽、已消费槽、槽内容改写、过期数量、撤销状态、topic 计数、revision 和导出字节数均可查看。topic/查询由宿主选择；没有订阅且没有显式查询时，不注入消息。上下文按相关标签/词项排序，最多四条，并标注截断，返回包含消息 ID、时间、revision、精确文本摘要的 `ContextSnapshot`。宿主应把该快照保存在私有 launch/receipt，不能只依赖 worker 可写日志。

各自的只读 `board.json` 可由已挂载的 worker 主动查询其有界、按参与者过滤的内容。因此 4 KiB 只约束**宿主自动注入**，不是模型全会话上下文或 worker 所有主动读取的总上限。索引不是全体广播，也没有消费者 ACK；慢读者不会阻塞发布、保留过期内容或无限扩大日志。导出含 `as_of` 和每条 `expires_at`，停止轮询后不能把旧文件称为实时新鲜状态。

## 持久化、隔离和生命周期

cohort ID 的前两位选择 authority shard；每 shard 有独立索引和锁。run ID 的 SHA-256 前两位选择另一个 ownership shard。注册只操作这两个局部索引，不扫描所有历史 cohort，也没有全局广播锁。固定锁顺序为 cohort shard 后 run shard。

run 的完整宿主身份先做原子持久化 reservation，然后提交 cohort 成员。若进程在两步之间退出，相同身份可继续，另一身份不能取得该 run。已初始化的 cohort authority 丢失或损坏时失败关闭，不重建为空白来重置配额。轮询把消息、去重、槽状态、额度和公平游标作为一个原子状态提交，然后才导出公共板和各成员的独立只读 view；提交后崩溃可按相同内容重建 view，不能重复发布或延长 TTL。游标逐成员访问同一槽位，再进入下个槽位；32 成员 cell 的首条消息可在一次轮询内处理。

槽位读取逐级 `openat` 固定目录，拒绝 symlink、hardlink、目录、FIFO 和超限普通文件，使用 nonblocking 有界读取。额外文件不枚举；非法输入只占固定槽记录，重复读取不会积累无界错误日志。这些是宿主解析和保留限制，**不是 worker 可写文件系统的总磁盘配额**；整个任务磁盘和进程资源仍由执行隔离层负责。

run 起初为 `active`，消息始终是尚未核验的提案。只有宿主从真实 workflow 结果确认成功后，才调用 `complete_run`：该调用额外扫描自身 16 个槽位，在同一次原子提交内收取退出前最后的提案并附上 receipt SHA，不改变全局公平游标；这仍不表示每条提案已经获语义核验。失败或不确定结果调用 `revoke_run`，不收取末尾未发布提案，已有内容保留审计但不再进入新的相关上下文。相同终态重放不改 revision 或重写完好的导出，仅修复丢失/旧版本 view；不同终态不能改写。

只有不存在 active run 时才能 `close_cohort`。待关闭后至少一个 TTL 且所有消息已过期，宿主可在按要求归档证据之后显式 `prune`。不会自动删除活跃 cohort 或实验记录；tombstone 和 run ownership 保留，旧身份不能重置额度。容量耗尽返回结构化 `CapacityExhausted`，并非清空历史的理由。宿主损坏/IO 缺口与容量耗尽分开记录；通信不可用不能导致已成功 workflow 被重试，也不应隐式阻塞独立研究任务。

## 验证边界

模块回归使用真实文件和子进程覆盖洪泛、消费者滞后、重放、额度、TTL、跨 cohort/身份冒充、reply 深度、危险文件、独立分片锁和两个持久化边界的进程退出恢复。独立模块测试不替代仓库最终 Rust/Bazel/Qualitygate，也不证明模型会遵循提案文本中的约束。

大规模逻辑身份/真实文件测试用于测量登记、局部查找、配额和磁盘开销，不计作真实模型调用、协作收益或 10,000 个同时运行的 worker。实际部署仍需要小规模真实 workflow/执行器隔离验收，并固定参与者、输入、通信权限与证据。正式效果实验不能通过开放公告板事后改变信息路径。

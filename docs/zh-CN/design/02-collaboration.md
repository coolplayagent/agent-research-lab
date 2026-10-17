# 02 身份、会话与目标

[文档首页](../00-index.md) · [本目录](00-index.md) · [English](../../en/design/02-collaboration.md)

OutcomeHub 拥有身份、群组、消息、目标和分工。通用协作不依赖研究配置；创建身份不会启动模型，没有 worker 时分工保持等待领取。

## 目标生命周期

目标包括稳定 ID、目标描述、验收标准、场景参数和 1–32 个成员分工。每个已领取分工时限为 30–43,200 秒，与会话寿命无关。临时群聊可以创建目标，公告板不执行目标。

不可变输入和每次尝试由存储服务与消息一起保存。创建、领取、提交和主持人决定产生事务性进度消息。worker 从 `ready` 到 `running`，再到 `submitted` 或 `failed`；所有分工成功提交后仍由主持人验收。失败、超时或被拒结果可显式重试，最多十次。领取 ID 隔离旧尝试，同一请求幂等。取消须等待活动分工完成或到期；活动目标阻止删除已分配成员或归档会话。扩展私信受众不复制或执行旧目标。

CLI 研究发现稳定研究身份并保留 IM 名称，生成只读、不可变 Job，冻结目标、验收、指令和有界历史。准入、来源、提示词、策略、SuperPOD、技能与隔离门禁继续生效；内置调度只执行关联目标任务。研究失败或未知状态不会自动完成目标。

## 目标与 worker API

宿主使用 `GET /api/im/scenarios`、`GET /api/im/goals?group_id=…&after=…` 和 `POST /api/im/goals`。写请求要求同源 Origin、JSON 和管理 intent 头。

`create` 的 `goal` 含 `id`、`group_id`、`title`、`objective`、`acceptance`、`scenario`、`parameters`、`assignments: [{person_id,instruction}]`、`max_seconds`。`accept`、`cancel`、`retry` 需要 `goal_id` 和观察到的 `revision`；重试还需要 `work_id`。

worker 以 `Authorization: Bearer <Agent grant>` 调用 `GET /api/im/work?after=…`，并向 `POST /api/im/work` 提交：

```json
{"operation":"claim","goal_id":"goal-123","work_id":"work-0","claim_id":"attempt-client-uuid","attempt":1}
```

```json
{"operation":"submit","goal_id":"goal-123","work_id":"work-0","claim_id":"attempt-client-uuid","attempt":1,"result":{"succeeded":true,"summary":"Compared both options and their limits","evidence":{"artifact":"report-id"}}}
```

使用已观察的 attempt，一次尝试的请求重放保持 claim ID。worker 必须遵守返回的 `deadline_ms`；核心拒绝迟到结果，但不能终止外部管理的进程。内置调度还把子进程批次限制在最早分工截止时间内。worker 授权不能管理目标或代表他人，研究分工仅接受宿主适配器结算。

## 会话不变量

临时群聊受众固定且无过期时间。扩展受众创建新会话；转为普通群保留日志、成员和确认游标。普通群新成员可读全部历史。归档阻止发言和成员变更，保留历史；公告板仅操作员可发布。

输入框 @ 成员可将用户原消息和有界讨论目标原子提交。重试保留请求身份及原目标输入；收到 Agent 消息不自动创建新目标。无 @ 的普通消息只进入聊天。此行为不同于研究文件提案板的纯文本 @，详见[通信协议](04-coordination.md)。

## 管理与消息 API

`collaboration-store` 实现身份、成员、凭证、消息、目标、尝试、幂等、重放与投递领域逻辑；生产持久库由独立存储服务持有。`collaboration-api` 负责 HTTP/SSE、登录、场景注册与前端，`collaboration-host` 负责启动装配，`apps/console` 提供研究适配器。

操作员写入 `POST /api/crystal/manage`，携带宿主会话、精确同源 `Origin: http://<listen-address>`、`Content-Type: application/json` 和 `X-Crystal-Intent: manage-crystal`。

| operation | 字段 |
| --- | --- |
| `person` | `person: {id, name, application_id?}`，更新稳定身份 |
| `create` | `group: {id, title, topic, kind, private, members}`，自动加入操作员 |
| `membership` | `group_id, person_id, add` |
| `update` | `change: {id, revision, title, topic, archived, pinned}`，乐观并发 |
| `fork` | `source, group, history_from`；`null` 不复制，`0` 复制全部 |
| `convert` | `group_id, revision, title` |
| `send` | `message: {group_id, request_id, text, reply_to?}`，操作员身份 |
| `grant` | `person_id, lifetime_seconds`，轮换 token，最长 24 小时 |
| `presence` | `person_id, state` |

Agent bearer 固定发言身份。`GET /api/crystal/inbox?after=…` 只列该 Agent 会话；`GET /api/crystal/history?group_id=…&sequence=…` 按成员权限读取。宿主另可使用 `/api/im/people`、`/api/im/scenarios`、`/api/crystal/view`、`/api/crystal/events`。目录每页最多 100 项，历史每页最多 128 条。

## 持久消息传输

| 接口 | 契约 |
| --- | --- |
| `POST /api/crystal/send` | Bearer 授权；`{group_id, request_id, text, reply_to?}`。身份由授权确定，持久提交后返回成功回执；相同 request ID 携带不同内容会被拒绝。 |
| `GET /api/crystal/stream?group_id=…` | Bearer 授权；SSE 事件为 `ready`、`message`、`revoked`、`fault`。先订阅后重放，重连默认从持久确认游标恢复，可用 `after` / `Last-Event-ID` 指定位置。 |
| `POST /api/crystal/ack` | Bearer 授权；`{group_id, sequence}`。消费处理后推进单调游标，socket 投递不等于处理完成。 |
| `POST /api/crystal/presence` | Bearer 授权；`{state: "online" / "chatting" / "busy" / "offline"}`。busy/offline 暂停投递，消息仍持久保留。 |
| `GET /api/crystal/view` | 操作员会话；分页群组、成员、历史和当前测量，`before` 翻阅旧消息。 |
| `GET /api/crystal/events` | 操作员会话；合并状态事件和所选会话消息。 |

存储采用单一有界 writer、SQLite WAL 和 `synchronous=FULL`，每次组提交最多 128 次发布，每个订阅群共享有界广播环。广播环是缓存，落后读者从索引日志重放。投递至少一次，消费者按序号去重。存储失败后禁止继续提交，恢复存储并重启服务。归档保留历史，不删除消息或释放存储配额。

当前准入上限：100,000 个传输身份、100,000 个群、每群 20,000 个成员、1,000,000 条成员关系、1,000,000 条持久消息、4,096 个排队写操作、256 条实时环记录及 20,100 个 SSE 订阅。人员目录维持 50,000 人和序列化 16 MiB 上限，完整 Soul 计入字节预算。这些是准入界限，不表示所有组合都已测试。打包服务允许 65,536 个文件描述符；当前无跨宿主复制或自动存储清理策略。

浏览器使用异步 SSE；控制器和 stdio-json 观察由 Linux 文件系统通知触发。队列溢出请求新快照并记录告警。网络刷新无需定时轮询，心跳、重连、界面时钟和通知合并仍使用计时器。大目录和成员列表分页，聊天渲染有界历史。

数据库 schema 增量升级，保留已有群 ID、凭证、消息和确认游标。回归覆盖私信受众隔离、历史及回复复制、转群与重启、公告权限、HTTP/认证边界、迁移、重放和背压。

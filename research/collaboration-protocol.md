# 八参与者协作研究 pilot

本协议研究 AI-SDLC 与长程 agent 的协作机制。所有参与者使用同一固定提交的既有 SuperPOD；实验计划、原始输出和评测留在私有运行目录，完整知识记录由宿主核验后统一写回 SuperPOD。这里不是第二个知识库。

## 契约与对照

一个实例包含两臂，每臂八个固定参与者、两轮，共 32 次有界 agent 调用（Codex exec task attempts），并不等于底层模型 API 请求数；一次 agent 调用可能包含多次模型轮次、工具调用或 provider 内部重试。每个 slot 在两臂中的模型、视角、角色提示词版本、任务时间上限和最大 claim 数一致。第一轮全部独立分析；第二轮 baseline 只读取自己的初稿，candidate 读取自己及环上后两位参与者的已提交初稿，先寻找反例再修订。保留少数观点，不要求共识。运行器仍是调度与效果权限的唯一所有者。

两臂均使用既有 `Task.dependencies`；运行器将工作流已确认且版本绑定正确的回执作为不可信研究证据传入。agent 不直接互相写文件或消息，不共享可变记忆。所有任务只读、`use_memory=false`、`next_tasks=[]`。不使用具备续作提案权限的 `implement` 角色。需要第三模型时由宿主在私有配置增加只读 `evaluate` 角色；harness 不改配置，不启动任务。

八个 slot 存在互相依赖，**不是八个独立统计样本**。32 次 agent 调用只构成一个团队层面的 A/B pilot；也不能证明八个进程曾同时运行。并发容量应通过独立运行区间验收。相同 agent 调用数与时间上限不等于实际 token 或费用相同；轮次顺序可能影响延迟，后续独立实例应预先交替 `first_arm`。

## 本地命令

先等待当前 worker 自然结束并停止控制器，再由宿主选择私有配置、共同 SuperPOD 来源和实验预算，依次 plan、批量注册和限定范围运行。普通异步 enqueue 不会阻止另一个控制器发现新任务；批量命令不提供全局暂停保证。应复用现有隔离边界与状态目录，避免另一个状态目录使原私有资料重新可见。示例配置的模型映射与 300 秒超时必须与实际控制器完全一致。

```sh
agent-research-lab --config /ABS/pilot.toml collaboration plan examples/collaboration/pilot.json --output /ABS/STATE/private/collaboration/pilot-001
```

`plan` 联网检查当前来源与已安装技能，冻结全部绑定、视角、问题、主假设和 DAG；注册带父版本的不可变提示词，不选择或晋级它们。重放同一计划是幂等的；来源、配置或请求变化必须使用新实验 ID。每个 task 必须实际读取声明的共同 SuperPOD 文件，引用固定 commit、路径、文件 SHA-256、行段及短原文。缺少可解析的共同知识内容引用属于协议无效，不能当成功结果。

使用 `collaboration enqueue MANIFEST --output /ABS/STATE/private/pilot-enqueued.json` 批量注册 32 个任务，再由现有控制器执行。批量命令只做一次最新来源/技能收集，对照冻结计划、配置和已注册提示词，预检全部既有任务，再按 manifest 顺序复用同一输入通过原有 enqueue 路径注册。既有匹配任务保留原始输入与提示词摘要；中断后重放只补缺项，任何陈旧基线或不匹配既有任务均拒绝。返回的 `worker_launch_requested=false` 仅说明该命令自身没有请求启动 worker；启动阶段仍重新检查 freshness。批次不是事务：普通 enqueue 竞态或后续 I/O 错误可能留下有效前缀，重放会检查后补缺，不回滚已注册任务。原有逐任务 `enqueue TASK.json` 继续可用。第一轮任务必须先注册，依赖才可引用；从 manifest 的 `tasks[].task.id` 提取 JSON 字符串数组，写入同一 `state_dir/private` 下的普通文件，再使用非 continuous 的 `run --max-seconds 3600 --task-ids-file /ABS/STATE/private/pilot-ids.json`，不传 `--seed`，执行完即退出。范围文件必须含 1..256 个唯一、已入队的 ID，所有依赖必须同时入选，任务配置摘要必须匹配当前运行配置；输入最多 64 KiB 且不能经过符号链接。当前 continuous 模式即使未传 `--seed` 仍会在空闲时补充研究任务，因此不能用于此固定预算 pilot。不得用额外研究调用污染本次预算。任务超时、失败与无效输出保留在计划分母中；不得只挑成功任务，也不得悄悄重试增加某一臂预算。每个计划任务固定 `max_attempts=1`，控制器的自动、恢复及手动重试都遵守此上限；历史未指定任务保留原有三次上限。需要重试时建立新实例并说明原因。任务时间上限由控制器真实 watchdog 执行。

```sh
agent-research-lab --config /ABS/pilot.toml collaboration collect /ABS/STATE/private/collaboration/pilot-001/manifest.json --output /ABS/STATE/private/collaboration/pilot-001/collection-001.json
agent-research-lab --config /ABS/pilot.toml collaboration collect /ABS/STATE/private/collaboration/pilot-001/manifest.json --adjudications /ABS/STATE/private/collaboration/pilot-001/host-grades.json --output /ABS/STATE/private/collaboration/pilot-001/adjudicated-001.json
```

显式任务范围同时筛选调度和 post-success 处理，并跳过全局 automation outbox。历史非空续作记录保持待处理；范围内任务若仍提出非空续作，宿主记录 `deferred_task_scope` 诊断并保留 Pending，固定范围正常结束后由无范围控制器继续处理。全局旧进程清理、租约与效果恢复仍执行，不会借范围漏掉旧进程。`run` 返回 `task_scope`（输入摘要、选定 ID 与延期续作 run），全局状态字段仍反映整个状态目录。

每次收集写新的不可变快照；未入队或未终态的任务保持 incomplete。收集器核对本地回执与 workflow 权威结果、task/DAG、配置、模型、提示词、源码、技能和 SuperPOD 绑定。陈旧或伪造的回执不会贡献 claim。历史结果可以收集，但不能因此绕过现有晋级时的最新版本检查。

## 观察与独立核验

新计划使用 `collaboration-pilot-v2`。`findings` 的每个字符串承载一个 JSON claim card，格式见 [claim-card 示例](../examples/collaboration/claim-card.json)。卡片的 `id` 是 **1..100 个 ASCII 字母、数字、下划线或连字符**组成的本地值，例如 `c1`；不包含 task ID 或 `#`。宿主自动生成 `task-id#c1`，只有跨卡字段 `counterexample_to`、`revises` 使用这种完整引用；只能引用本轮已提供的依赖，修订只能指向自己的初稿。`change`、反例关系和知识建议都只是模型提案。

已冻结的 `collaboration-pilot-v1` 计划仍按原始任务文本、DAG 和摘要验证、收集，结果保留 v1 标识。v2 不改写旧计划或报告，不接受旧报告中的不安全 ID，也不为旧实例增加尝试。协议变更后的验证使用新实验 ID；未知协议继续拒绝。

结果中 `scheduled_calls` 等 `*_calls` 指计划的 agent 调用次数，不是 provider API 请求计数。收集器分开记录：

- 引用可解析性：固定 Git 对象的文件摘要、行段和短原文是否匹配。存在原文不等于支持论断。
- 字面重复：最终 claim 经空白和大小写归一后的重复数。引用同一来源不直接算重复工作；语义重复与实际无效工作需要宿主另行核验。
- 反例与结论改变：先记录提出的关系，只有宿主独立判断后才计有效反例、正确收窄或撤回。
- 成本诊断：对所有已注册调用（包括失败与无效报告）读取实际 Codex `turn.completed` 的 observed usage，未执行或缺失保持空值。该日志位于 worker 可写目录，摘要绑定不使它成为独立账单证据，不支持自动效率晋级。运行时上限不是实测执行时长。

宿主评分见 [adjudications 示例](../examples/collaboration/adjudications.json)，必须位于 `state_dir/private`，绑定计划及 claim/回执摘要，并给出独立来源与理由。建议先遮蔽臂标识再逐条检查。团队层面的主要比较是去重后独立支持的结论及有效纠错目标变化，同时报告误引、新错误、失败和缺失证据。`introduced_error` 由宿主比较首轮后填写；未评价保持 null，并同时报告已评价数量，不能把缺失当作无新增错误。重复纠正同一初稿 claim 只贡献一个 `unique_counterexample_targets`。不要把更多字数、更多批判句或模型自报改变当成质量提高。

claim 的 `knowledge_update` 指向**已有 SuperPOD 文件**。收集器只输出 `pending_host_review` 提案；宿主检查公开范围、证据、冲突和现有节点后，通过现有知识发布流程提交合并，并由唯一索引写入者更新。agent 不并发 push，不建立本地替代 KB。

## 提示词进化与后续实验

先用真实 trace 找协议缺口，再形成固定实例集。一轮 pilot 不生成成功标签或 `TrialObservation`，不自动调用 `evolution evaluate/promote`。正式晋级仍需至少三任务族、每族三次独立团队成对试验、相同正式策略与固定知识版本，以及原有收益和严重回归门槛。新能力还需要保留任务族与消融证据。

下一轮分别消融 peer 证据、质疑步骤和观点差异；保持其他模型与预算不变。要试验提示词进化，注册带父版本的新候选，每次只改变预先指定的角色策略，在开发实例上诊断，再由隔离的独立评测检验。模型或来源版本更新后重新建立基线。模拟协议回归只证明字段、依赖和收集规则，不能宣称研究能力提升或能力涌现。

## 官方实践依据

- OpenAI [Orchestration and handoffs](https://developers.openai.com/api/docs/guides/agents/orchestration)：有界 specialist 与最终责任归属分开；本项目的宿主控制器承担调度和验收责任。
- OpenAI [Evaluate agent workflows](https://developers.openai.com/api/docs/guides/agent-evals)：先诊断 trace，再建立可重复的数据集评测。本实现使用本地 Rust 数据格式，不依赖托管 Evals 或 managed prompt 对象。
- Anthropic [How we built our multi-agent research system](https://www.anthropic.com/engineering/multi-agent-research-system)（2025-06-13）：明确目标、输出、工具和范围，约束研究投入并检查引用；其产品内部对比不是本项目八 agent 优势的证据。
- Anthropic [Harness design for long-running application development](https://www.anthropic.com/engineering/harness-design-long-running-apps)（2026-03-24）：先确定可检验契约，分开生成与评价，逐组件消融，并随模型能力变化重测架构。文中的展示也不能代替本项目的等条件实验。

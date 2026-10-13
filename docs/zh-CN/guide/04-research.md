# 04 研究配置与运行

[文档首页](../00-index.md) · [本目录](00-index.md) · [English](../../en/guide/04-research.md)

## 配置与工具前提

Linux 研究宿主需要 Git、Bazel、Rust、支持用户命名空间的 Bubblewrap、已配置的执行器与 GitHub CLI。先完成[快速开始](01-quick-start.md)，再复制 [examples/lab.toml](../../../examples/lab.toml) 为不进入 Git 的 `local.toml`。长期运行使用 `bazel build -c opt --lockfile_mode=error //:agent-research-lab //:package`。

运行研究前编辑 `local.toml`：设置工作区、私有运行目录、已有 SuperPOD 检出、Codex 可执行文件、已发布的 workflow CLI 和 7 个工具绑定。示例路径来自开发机器，仓库路径必须指向已有本地 Git 检出。模型/API 认证通过已安装工具配置，凭证不得写入跟踪的示例配置。

## 最新版本与工具证据

生产默认 `require_latest = true`。`skills_manifest` 必须指向宿主持有的绝对 JSON 路径，记录全部 7 个已安装技能目录、运行时路径、目录与运行时 SHA-256，以及最新发行版标签/ID或默认分支源码提交。

上游存在稳定发行版时，安装较新的本地源码构建还需要 `source_qualitygate: {path, sha256}`：绑定该源码提交、完整、非空且通过的 full diff 报告。保留构建来源，并明确标注开发构建；缺少这些证据时，仍须满足原有发行版要求。

精确结构由 `freshness::SkillsManifest` 和 `SkillSnapshot` 定义，目录摘要算法由 `freshness::skill_tree_digest` 定义。只有完成实际安装和验证后才能填写证据。5 个工具使用上游技能；`skills/` 中另外 2 个项目自有适配说明覆盖缺少上游技能的 Computer Use 与 Relay Memory。运行时二进制在本地安装时打包，不在此仓库跟踪。

```sh
bazel-bin/agent-research-lab skill-digest /absolute/path/to/installed-skill
bazel-bin/agent-research-lab --config local.toml refresh
```

目录摘要不能替代发行版或下载校验。控制器核对 GitHub 当前稳定发行版和安装字节，拉取各远程默认分支，并建立独立只读源码工作树，不切换或覆盖开发者分支。最新发行版与最新源码分别记录；旧发行二进制出现问题时，必须在固定的当前源码构建上复现，才能将其视为当前源码缺陷。

网络不可用、技能变更或上游推进都会阻止任务准入，不回退到过期本地检出。发行版变化后需安装、验证新包并更新宿主清单。独立技能及仓库检查每组最多并发 4 项，共用同一绝对宿主时限；已启动检查结束后才返回错误，部分结果不会替换当前输入清单。

每个空闲准入批次在启动同一实验批次的最多 8 个 Agent 前验证新鲜度，并冻结源码快照。独立心跳在控制器准备其他 Agent 或处理结果时续租。启动和结算时再次校验技能及运行时字节，并发安装变更会使结果失效。源码、技能、配置或研究输入变化产生新的每日任务 ID，旧实验保留为历史证据。连续模式空闲时每 15 分钟检查新批次。`require_latest = false` 仅用于离线合成夹具，不能证明研究基于当前版本。

| 工具绑定 | 用途 |
| --- | --- |
| `workflow-cli` | 持久化作业状态、领取、租约与证据结算 |
| `relay-knowledge` | SuperPOD 地图校验、路由和精确提交索引 |
| `into-markdown` | 研究素材转换 |
| `qualitygate-cli` | 完整且绑定快照的交付验证 |
| `computer-use-cli` | 独立测试桌面/浏览器任务 |
| `relay-memory` | 显式本地存储的隔离任务记忆 |
| `repo-sandbox` | 已配置执行环境中的任务 |

桌面/沙箱目标准备、模型探测与诊断见[部署与维护](07-operations.md)。

## 在群聊中启动研究

在同一服务和私有状态中启用研究适配器：

```sh
bazel-bin/agent-research-lab --config local.toml serve --listen 127.0.0.1:8090 --max-seconds 43200
bazel-bin/agent-research-lab --config local.toml serve-link
```

首页保持 AI-IM。适配器通过稳定 ID 发现已配置研究身份，保留在 IM 中设置的名称。创建群聊，在目标中选择 **CLI 研究**、配置的仓库和研究成员。准入失败会将场景标为暂不可用，并在私有 `private/im-admission/` 中保留有界诊断；界面仅显示稳定状态和结果码。每个分工生成不可变、只读的研究 Job，结果连同源码、提示词、策略及 SuperPOD 绑定返回群组。结果需要主持人显式验收；实现与发布仍受原有 CLI 门禁约束。

`/apps/research/` 提供同一批任务、历史运行和研究身份配置的详细观察界面。以下说明涉及该研究应用及原有显式研究命令；IM 负责会话创建、成员和群目标生命周期。

### 数字人、执行技术与会话

在研究观察界面选择数字人、图节点或消息作者，会打开独立会话窗口，保留群聊。公开消息和结果可见，连续工具操作折叠为过程卡片，内部展示命令、输出及规范化 stdio JSON。长消息按需展开，错误保留摘要；历史页可向前/向后翻阅。查看旧记录会暂停实时跟随，返回最新页后恢复；图、共享聊天和会话窗口可放大，关闭窗口回到原群聊。已展开的过程详情在更新和调整窗口尺寸时保留。

提及和数字人面板可查看模型、后端、任务、记忆、所需工具、活动及证据。每个数字人拥有唯一显示别名，在消息、提及、图节点和会话窗口中一致展示。身份保存在私有状态的 `people/directory.json`，使用非阻塞锁和原子写入；旧 `dashboard/personas.json` 可导入，保留既有人员、任务、会话和记忆。

普通研究跨主题群组复用固定研究者、批评者和综合者。模型或任务变化不改变人员 ID。当前群内人员去重；人员窗口列出全部关联任务与尝试，包括最新 512 个任务观察范围之外的历史。告警和恢复记录显示在独立系统消息区，页内历史有界。

研究身份配置管理 Soul 和执行偏好：可创建固定、临时或研究人员；将临时/研究人员转为固定人员时保留 ID、历史和记忆；可编辑唯一姓名、Soul、用途，并为任务角色选择固定默认人员。专长描述研究视角，不绑定模型或授予权限。同一固定人员可以承担不同角色，但实现的独立评审必须由其他人员承担。配置修订使用乐观并发检查，只影响新研究批次。

数字人拥有身份、Soul、记忆和会话历史，编码 Agent 是可替换的执行技术。人员执行页可选择已配置执行器和模型，或继承任务默认值。宿主将偏好冻结进新 Job，并分别记录实际模型、执行器规范和可执行文件摘要。修改偏好不重绑现有任务、重试、记忆目录或历史会话。未知模型/执行器或能力不足明确失败。目录中声明的能力不等于提供商兼容性或质量证据。

`[models]` 保持任务角色默认映射。可选 `[backend_models]` 将执行器 ID 映射到模型列表，添加技术模型无需新增角色或数字人，示例见 `examples/backend.toml`。未显式列出时接受原有角色默认模型。研究身份界面只展示 ID、模型和声明能力；程序路径、参数和凭证环境名保留在宿主。

会话以 `task.id` 标识，每次尝试有独立 `run_id`。二者进入历史，JSON bridge 请求同时携带冻结的数字人身份。水晶球经授权任务/会话端点读取，与具体编码 Agent 实现解耦；替换执行技术不改变消息权限或私聊范围。原生 Agent 活动作为人员会话内的折叠过程详情。

手动注册任务可通过 `"persona_id":"person-…"` 指向既有人员；省略时保留原有任务/正式实验的隔离行为。本地 HTTP 管理接口仅接受有界、同源、带明确 intent 头的 JSON 请求，不提供任意命令执行。

研究观察界面的群组、定向聊天和人员列表可折叠，置顶群组优先。侧栏可隐藏或拖动边缘调整宽度，聚焦边缘后可用方向键调整，双击恢复默认；共享聊天折叠后保留会话和滚动位置。浏览器保存这些显示偏好。左侧三个列表共享剩余高度，可拖动水平分隔线调整比例；窄屏保留堆叠侧栏和可调整的人员浮层。

### 记忆、提示词与证据

每个人有独立的 relay-memory 目录 `people/memory/<person-id>`。记忆页调用配置的 CLI 进行回忆、统计及显式用户笔记操作，不另建记忆引擎或直接编辑 SQLite。任务注册时冻结有界 `prepare` 上下文，将上下文、数据包、可执行文件摘要及身份修订写入 Job 和提示词摘要。Agent 不挂载共享人员记忆，重试使用相同的已捕获上下文。独立正式实验不进入此可选身份记忆路径；任务局部记忆保持隔离。

权威 workflow 成功后，宿主通过 `remember` 写入简短结果检查点及源码、提示词、策略/配置、SuperPOD 和回执引用。公开结果和用户笔记可成为后续上下文；私聊、原始运行、凭证和保留集不复制。完成日志防止重复成功写回。CLI 退出不明确时保留未知状态并显示系统消息，须对照 relay-memory 核对后才能重试。个人记忆只是历史证据，不构成新授权或研究结论证明。

独立 Soul 与现有任务角色指南分别展示。角色指南来自冻结的实验室源码或注册提示词版本，使用好奇研究、审慎质疑、务实实现和公平评估的对话语气。Agent 可通过简短公开理由及可检验问题表达关注、兴趣和不确定性；界面保留原文，不为历史消息补写情绪或证据。按需身份端点只读取已知任务，角色文本上限 16 KiB，Git 读取上限 2 秒；历史来源缺失时明确报告。

## 显式运行有界研究周期

以下命令用于主动发起原有 CLI 研究流程：

```sh
bazel-bin/agent-research-lab --config local.toml enqueue examples/task.json
bazel-bin/agent-research-lab --config local.toml run --max-seconds 900
bazel-bin/agent-research-lab --config local.toml status
bazel-bin/agent-research-lab --config local.toml report
```

`enqueue` 将任务 ID 绑定到输入、源码提交、SuperPOD 提交、提示词和配置；相同 ID 不能绑定不同输入。每个任务使用隔离工作树，依赖接收校验过的完成报告。默认 `write: false`。`required_tools` 声明准入依赖，也适用于后续提案；`use_memory` 启用任务局部 relay-memory，保留有界检查点和不可变回执引用。完整源码/技能快照留在私有回执中。`prompt_version` 显式选择已注册提示词。

`seed` 或 `run --seed` 添加每日独立研究、批评和综合任务，覆盖交付、记忆/恢复、协作和桌面/沙箱操作。综合任务可提出允许仓库中的有界后续任务，实现可请求只读评审。控制器限制提案数量与深度；模型提案不授权安装、远程写入、发布或合并。

```sh
bazel-bin/agent-research-lab --config local.toml pause
bazel-bin/agent-research-lab --config local.toml resume
bazel-bin/agent-research-lab --config local.toml run --max-seconds 900
```

`pause` 请求停止准入并取消子进程，`resume` 清除暂停标记。重新运行会核对持久尝试与实际进程身份。只读失败有界重试，`retry TASK_ID` 可显式重试符合条件的只读任务。中断写入保留不确定结果，先核对再重试；恢复不保证模型进程内部指令级续接。

保留的任务启动记录复用原超时和输入封套，即使新控制器默认超时改变。成功 workflow 回执可重建崩溃后的持久完成工作；后续任务准入可重放，未核对的记忆写回继续标为未知。完成工作、新鲜度检查和播种共享每日预算及 `run --max-seconds` 时限。

默认最多并发 8 个 Agent，每个 Asia/Singapore 自然日最多使用 43,200 秒活动墙钟时间。并发任务共享该时长，它不是金额或 token 上限，预算状态跨重启保留。先运行有界周期并检查结果，再显式启动连续播种：

```sh
bazel-bin/agent-research-lab --config local.toml run --continuous --seed
```

持续播种须由显式命令启动；不能与使用同一状态的统一服务并行运行。服务模板及观察诊断见[部署与维护](07-operations.md)。

## 显式持续 CLI 研究

宿主可在既有会话配置持续研究。可选私有文件 `state_dir/private/research-campaign.json` 包含 `schema_version: 1`、`id`、`group_id`、四个不同 `person_ids`、四个已配置 CLI `repositories`、`objective`、`acceptance`、四条成员 `instructions`、`max_seconds`、`pause_seconds`、`failure_backoff_seconds`、`max_rounds`。先创建群与研究身份，至少提供四个 worker 名额并满足普通研究准入。文件缺失时不运行 campaign。

每轮创建四个并行分工的不可变目标，并轮换下一个 CLI。重启从既有目标恢复游标，重复准入幂等；完整策略摘要冻结进目标和研究提示词。同 campaign ID 的策略不可改写，修改需新 ID。失败/超时推迟下一轮，既有每日预算、源码和 SuperPOD 绑定、新鲜度、隔离及存储容量继续约束执行。

群任务卡展示四位成员实际状态，可展开、刷新公开过程并向前翻页。过程请求先通过存储 API 解析当前目标、分工和尝试，再读取有界执行投影，不暴露原始日志、推理、提示词或凭证。成员贡献和结果证据留在群记录。

删除私有 campaign 文件停止后续准入，已领取分工遵守原时限完成。取消 campaign 目标停止后续轮次，仍适用活动分工取消规则。提交结果只允许继续探索，不代表验收、版本晋级、写权限或合并批准。交付需单独由宿主复现、独立评审、运行仓库测试及完整 Qualitygate，并满足精确 head 的远端审查与 CI 门禁。

## 准备批次与简明状态

研究调度器在没有活动或保留启动记录的空闲窗口，先准备任务选择范围内的下一批任务，再逐个领取执行租约。批次受并发上限、依赖、研究版本和探索轨道约束；失败的工具准备不占执行名额。桌面依赖与任务私有构建缓存的串行准备可能延迟首个任务启动，准备耗时计入既有运行与每日预算，启动前仍复核版本、暂停和剩余时限。

`status --summary` 适合重复监控，仅返回 `paused`、`budget`、按同一控制器规则分类的 `summary`、`total_jobs` 和 `details_included: false`，不包含逐任务详情、提示词或工作流回执。观察仍按任务顺序读取，并非原子快照；需要诊断详情时使用 `status` 或 `report`。

```sh
bazel-bin/agent-research-lab --config local.toml status --summary
```

## 有界批量注册

`agent-research-lab --config local.toml enqueue-batch request.json --max-seconds 300` 按序注册 1..32 个普通原生 Task。请求为 `{"schema_version":1,"batch_id":"owned-batch","tasks":[...]}`，必须是最多 1 MiB 的普通 JSON 文件。依赖须已存在或出现在批次前方，必须启用最新基线模式。

每次调用只采集一次最新输入。私有不可变计划 `state_dir/private/enqueue-batches/<batch_id>/plan.json` 在注册副作用前冻结 Task 选择、人员记忆、配置和全部源码/技能/知识输入。重放使用相同规范请求与冻结选择，但仍重新采集并拒绝真实绑定漂移；只有仓库/技能获取时间不参与身份比较。实际序列化计划最多 16 MiB。

注册持有批次锁和原生 enqueue 锁，在首次新增前校验全部既有绑定和中断工作树。失败保留部分状态，以非零结构化响应返回 `code` 与 `registration`；私有回执区分 Job 已记录和 workflow 注册已确认。回执保存失败时观察结果为 `receipt_persisted:false`。诊断后重试同一请求，不自动重试、回滚或覆盖 Job。注册中提交 workflow bundle，owner 拒绝可能留下已注册前缀。

既有尝试通过 owner `verify`、`history-at` 和纯 `kernel check` 确认；不修复推进中/活动尝试、不重置为 attempt 1、不取租约或启动 worker。缺少或不支持 owner proof 保持不完整。注册完成不等于任务成功，确认的 owner run 仍可能运行、失败或取消。并发控制器可观察前缀，不构成原子 cohort 准入。

1..600 秒额度由获取、准备和原生调用共享。检查点停止后续副作用，同步哈希/文件系统同步不能被进程内截止时间抢占。普通 `enqueue`、协作实验、seed 和 run 准入行为不变。

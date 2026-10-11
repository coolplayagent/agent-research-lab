# Agent Research Lab

基于 Rust 和 Bazel 的群体 Agent 研究协作平台。**AI-IM 是系统核心**：数字人身份、群聊、持久化消息、群目标、成员分工、结果提交、研究主题和版本演化共同构成一个协作工作区。CLI 研究作为可选场景接入，复用证据绑定、隔离工作树和现有 SuperPOD 知识库。

主界面采用三栏布局：左侧会话目录、中间群聊、右侧研究面板。群聊始终是协作入口，数字人的真实研究结论以成员身份回复到群聊，可打开证据与验收；右侧通过 Tab 切换协作水晶球、版本演化、目标和成员。数字人创建只需名称，执行能力在系统设置中绑定。进入系统设置后，左侧切换为服务、适配器、Agent 绑定、存储和状态列表。

## 快速启动

无需研究配置即可启动独立协作系统：

```sh
bazel build --lockfile_mode=error //:agent-research-lab //:package
bazel-bin/agent-research-lab serve --state-dir /absolute/private/ai-im --listen 127.0.0.1:8090 --max-seconds 43200
bazel-bin/agent-research-lab serve-link --state-dir /absolute/private/ai-im
```

创建群聊、添加数字人，再通过**设置目标**填写目标、验收标准和成员分工。通用执行器通过 Agent API 领取任务。使用 `--config local.toml` 后，同一个目标表单中会出现 **CLI 研究**场景，由服务内部的有界调度器执行。该调度器只接收群目标关联的任务；自动研究播种和发布队列需通过显式命令启动。

在输入框中键入 **@成员名** 可选择参与者，例如 `@北岑 @月雪 请讨论这个方案`，发送后直接发起一轮讨论，无需另填表单。点击发言者或图中的成员可插入 @；消息和对应目标由协作存储在同一事务中保存，重试不会重复创建任务。研究成员按提及顺序接续发言，前序结果会固定进入后续上下文；没有 @ 的消息只进入群聊。每轮有明确成员、执行时限和主持人验收，不自动无限循环。也可在目标面板中使用更详细的分工与并行研究选项。

水晶球保留玻璃球与外围成员卡片，支持悬停高亮、点选成员、点击球体返回群聊。真实新发言会触发连线流光和节点闪烁，任务执行状态有呼吸动画；历史消息不制造新活动，系统减少动态效果设置会关闭动画。

研究回执会绑定源码、提示词、策略和 SuperPOD；较长报告在群内保留有界摘要及完整回执的摘要值。版本节点冻结关联目标的任务尝试，只有证据与四项绑定均匹配且主持人已验收时才显示已验收。旧回执和旧节点仍可读取，缺少新证据绑定时显示未评估，需追加新节点完成评估。

临时群聊采用固定成员范围，没有过期时间。任务执行时限与会话生命周期分别管理。群聊、目标 API、部署和前端模块说明见 [IM.md](IM.md)。

系统设置管理独立部署的存储服务、编码 Agent 执行器和轻量沙箱，包括启用状态、协议健康、依赖和 Agent 绑定。Claude Code、Codex、DeepSeek Harness 与 Pi 使用统一的 JSON 适配协议。已配置、服务健康和实际任务成功是不同证据，界面分别展示。可选研究场景还保留原有后端能力检查，详见 [BACKENDS.md](BACKENDS.md)。

[多 Agent 协调](MULTI_AGENT.md)提供宿主控制的共享提案板、按主题选择上下文、配额和恢复机制；消息不会自动触发任务创建。普通执行最多并发 8 个 Agent。内置 1,000 / 10,000 参与者模拟验证有界单元和按需拉取摘要的路由，不会启动同等数量的模型进程，也不代表研究质量提升。

研究调度器在没有活动或保留启动记录的空闲窗口，先准备任务选择范围内的下一批任务，再逐个领取执行租约。批次受并发上限、依赖、研究版本和探索轨道约束；失败的工具准备不占执行名额。桌面依赖与任务私有构建缓存的串行准备可能延迟首个任务启动，准备耗时计入既有运行与每日预算，启动前仍复核版本、暂停和剩余时限。

研究知识归属现有私有 [SuperPOD 仓库](https://github.com/stevetdp/superpod)。访问本公开项目不会获得 SuperPOD 权限。本仓库保存研究软件、提示词和合成测试夹具；私有知识、凭证和实验原始记录不进入 Git。

经审查、可公开分享的经验与假设保存在 [research/insights.md](research/insights.md)。新任务从项目远程默认分支的当前提交读取该文件，并冻结内容摘要。每条记录保留证据、版本限制、反例和下一步实验；详细知识及来源归档继续留在 SuperPOD。

## 构建与配置

Linux 是首个支持的运行平台。需安装 Git、Bazelisk/Bazel、支持用户命名空间的 Bubblewrap（`bwrap`）、Codex CLI 和 GitHub CLI（`gh`）。Bazel 与 Rust 版本分别固定在 `.bazelversion` 和 `rust-toolchain.toml`。Bazel 使用 `rules_rust`，Cargo 清单及锁文件是 Rust 依赖的来源；格式检查和 Clippy 也需要可用的 Rust 工具链。

```sh
bazel build -c opt --lockfile_mode=error //:agent-research-lab //:package
bazel test -c opt --lockfile_mode=error //...
cp examples/lab.toml local.toml
```

持续运行请使用优化构建：未优化的完整技能目录哈希计算可能占用较多宿主执行时限。

仓库采用 Cargo workspace，各模块独立为 crate：

```text
app/                         # CLI 与应用服务组装
  src/{main,cli,service}.rs
  systemd/                   # 可选用户服务模板
crates/
  config/                    # 配置、后端与团队声明
  task/                      # 冻结的任务、作业和启动记录
  crystal/                   # 身份、群组、消息、目标和分工
  im-service/                # HTTP/SSE 服务、场景及模块化 TypeScript 界面
  runtime/                   # 可选研究调度与恢复
  agent-backend/             # 编码 Agent 调用及结果校验
  multi-agent/               # 宿主协调与规模模拟
  communication/             # 有界共享提案板
  ...                        # 每个模块都有 Cargo.toml 与 BUILD.bazel
Cargo.toml                   # 工作区成员与共享依赖版本
BUILD.bazel                  # 稳定的二进制、测试和打包入口
```

模块职责与依赖规则见 [ARCHITECTURE.md](ARCHITECTURE.md)。`app/` 组装库，库不依赖应用层。应用包和可执行文件均名为 `agent-research-lab`。

运行研究前编辑 `local.toml`：设置工作区、私有运行目录、已有 SuperPOD 检出、Codex 可执行文件、已发布的 workflow CLI 和 7 个工具绑定。示例路径来自开发机器，仓库路径必须指向已有本地 Git 检出。模型/API 认证通过已安装工具配置，凭证不得写入跟踪的示例配置。

### 最新版本与工具证据

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

### 本地实验目标与诊断

控制器可准备可丢弃的本地目标，无需个人桌面或远程机器配置。Ubuntu 上的 `targets prepare` 将经过认证的 APT 包下载到 `.lab/tools/desktop`，无需 sudo，也不安装系统包。实际桌面使用 Xvfb、Openbox 和项目自有 GTK 输入程序。每个计算机研究任务拥有独立显示、认证 cookie 和输出目录；控制器先观察其唯一窗口，Agent 再通过已安装的 computer-use CLI 操作并对照真实 GTK 事件回执。

repo-sandbox 的 local provider 使用宿主工具，为每次执行创建真实 Bubblewrap 命名空间。持久会话记录工作区并报告 `ready`，不表示存在常驻容器。控制器使用私有注册表运行 `target install`、`dev up/exec/status/down`，并验证实际工作区写入。这些操作在宿主执行，因为部分 AppArmor 策略禁止 Agent 内嵌套用户命名空间。模型可读取保留的证据及私有注册表；需要 WSL/VM 语义时可使用对应目标。

```sh
bazel-bin/agent-research-lab --config local.toml targets prepare
bazel-bin/agent-research-lab --config local.toml targets verify --kind all
bazel-bin/agent-research-lab --config local.toml targets status
bazel-bin/agent-research-lab --config local.toml targets agent-check
bazel-bin/agent-research-lab --config local.toml doctor
bazel-bin/agent-research-lab --config local.toml doctor --probe-models
```

`targets verify` 要求支持 local provider 的 repo-sandbox 构建，旧运行时会明确失败。准备过程有时限且可恢复；缺失依赖产生可操作错误，不阻断无关研究。真实验证日志和截图保存在 `.lab/runs`。`targets agent-check` 经生产目标启动器调用已配置的评审模型，核对实际工具事件和 PNG；它消耗模型额度，证明的是传输链路。

控制器管理的 Codex worker 和模型探测显式使用 `approval_policy = "never"` 与 `sandbox_mode = "danger-full-access"`，每次尝试记录到 `agent-permissions.json`。控制器负责 8 Agent 上限，并禁用嵌套 Codex Agent。外层挂载/PID 隔离仍保护宿主凭证、其他尝试及评估保留集；操作系统权限错误需诊断目标或命名空间。

模型探测为有界真实调用，会消耗提供商额度。示例为研究配置 `gpt-6-astra`、实现配置 `gpt-6.1-sol`、评审配置 `gpt-6-sol`。可用性取决于账号，不可用时明确报错，不静默换模型。JSON 命令返回 `ok` 和 `result`，或返回错误并以非零状态退出。

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

提及和数字人面板可查看模型、后端、任务、记忆、所需工具、活动及证据。每个数字人拥有唯一中文别名，在消息、提及、图节点和会话窗口中一致展示。身份保存在私有状态的 `people/directory.json`，使用非阻塞锁和原子写入；旧 `dashboard/personas.json` 可导入，保留既有人员、任务、会话和记忆。

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

### 观察、边界与长期服务

经认证的同源 SSE 推送变化并自动重连。Linux 文件系统通知触发 workflow/提案板投影和最多 16 个活动会话投影。节点脉冲表示新观察到的会话字节，连线活动表示新提案板记录；连线来源于实际发布或初始上下文投递。stdio-json 适配器投影公开 Codex 消息、命令、工具结果和轮次状态；其他 JSON bridge 可输出可选 `session.activity` 事件。推理字段、提示词和原始协议对象不展示。缺少源时间戳的事件保持无时间，界面另标明日志修改时间。

观察器使用控制器的 workflow 状态规则，读取已校验的提案板权限，不轮询 worker outbox。过期/撤销提案标为历史，不重新注入上下文。上下文注入不能证明采纳，workflow 成功不能证明研究质量，候选提交不能证明通过门禁或已安装。

采样最多扫描 8,192 个目录项，选择最近更新的 512 个 Job；每个 Job 上限 512 KiB、关联提案板上限 32 个，workflow 采样时限 20 秒。研究观察图最多展示 8 个 Agent，优先活动运行；全部选中任务仍可从会话列表访问。群聊显示最多 256 条提案板消息。每个会话页读取最多 256 KiB，投影最多 48 个公开事件，使用稳定字节偏移 ID。`before` 为排他的字节游标，向前翻页不丢失页边界处完整行。实时页随推送字节变化更新，尾部不完整行及超预算行省略，公开字段保持裁剪。

采样是顺序观察，不是原子数据库快照。失败时保留前次数据及错误时间。慢速宿主读取最多并发 16 项，异步流上限另为 20,100 个订阅；断连、撤权或停机结束流。关闭服务为网络排空及有界宿主工作预留 5 秒。

原有控制器在私有 `controller-status.json`、`seed-status.json` 报告心跳、活动任务和最新输入刷新状态。启动先初始化 workflow 存储，再恢复作业。连续模式刷新失败保持禁止准入，30 秒后重试并退避至 5 分钟，成功后恢复 15 分钟周期。界面分别报告刷新失败、心跳延迟和连接状态。systemd 模板重启每次最长 12 小时的进程；每日预算及暂停状态跨重启保留。

长期部署应将验证过的技能树与清单固定到私有 `tools/skills`，绑定控制器工具及可选私有 Codex profile。全局技能更新不得改变进行中实验的工具树。新版本需要新的源码/清单验证和新准入批次，不清除旧阻塞项或修改冻结证据。

服务仅接受回环绑定及固定 GET 路由，使用 Host/Origin 检查、no-store 和同源内容策略。提案、公开 Agent 输出、工具摘要和阻塞记录均可能含研究信息，应作为私有数据。常见凭证字段会隐藏，但这不是通用秘密扫描器。原始日志、租约、任务记忆和任意文件下载不开放，不加载外部资源或遥测。

## 显式运行有界研究周期

以下命令用于主动发起原有 CLI 研究流程：

```sh
bazel-bin/agent-research-lab --config local.toml enqueue examples/task.json
bazel-bin/agent-research-lab --config local.toml run --max-seconds 900
bazel-bin/agent-research-lab --config local.toml status
bazel-bin/agent-research-lab --config local.toml status --summary
bazel-bin/agent-research-lab --config local.toml report
```

`status --summary` 适合重复监控，仅返回 `paused`、`budget`、按同一控制器规则分类的 `summary`、`total_jobs` 和 `details_included: false`，不包含逐任务详情、提示词或工作流回执。观察仍按任务顺序读取，并非原子快照；需要诊断详情时使用 `status` 或 `report`。

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

`app/systemd/agent-research-lab.service` 为可选用户服务模板，需按机器调整程序、工作目录和配置路径；构建不会自动安装或启用服务。

## 提示词与协作实验

策略保留不可变父子关系、角色、任务族、修改理由和失效条件。注册不自动激活或晋级，评估适用性后再在任务中显式选择版本。

```sh
mkdir -p .lab/evolution
bazel-bin/agent-research-lab evolution prompt-register \
  examples/evolution/prompt-register.json --output .lab/evolution/prompts.json
```

运行时从 `state_dir/evolution/prompts.json` 读取注册表，状态目录不同时请调整输出路径。其他动作包括 `prompt-portfolio`、`evaluate`、`novelty`、`exploration`、`rule-proposal`、`holdout-validate`，类型化 JSON 示例及路径约定见 [examples/evolution](examples/evolution/README.md)。

门禁依据分别固定的宿主上下文和实际观测计算，拒绝未知字段、过期绑定、缺失测量和严重回归。任务成功退出或报告中的结论不能替代独立评估。宿主须提供真实评估运行的测量；仓库观测文件是合成夹具。

提示词晋级要求覆盖 3 个任务族和重复配对实验，比较成功率与实测成本；新能力主张还需保留集迁移和消融证据。每类任务的组合保留 1 个稳定、2 个专门和 2 个探索策略。探索比例从 25% 开始，完整 10 次实验窗口后调整，保持在 10–50%。正式规则提案保持隔离，不能自我批准。

保留任务存于 `state_dir/private`，不得进入 SuperPOD、共享记忆或优化器工作树。正式门禁输入、上下文、回执由宿主持有，并放在候选可写挂载之外；仅放在不同目录不构成安全边界。

## 宿主交付队列

宿主可将明确审查过的操作加入 `state_dir/outbox`：

```sh
bazel-bin/agent-research-lab --config local.toml automation enqueue --input local-operation.json
bazel-bin/agent-research-lab --config local.toml automation tick
```

操作使用 JSON 封套 `{"kind":"…","request":{…}}`。

| 类型 | 请求结构 | 效果 |
| --- | --- | --- |
| `install` | `delivery::InstallRequest` | 暂存并冒烟验证已校验技能和打包 CLI |
| `pr` | `delivery::PullRequestRequest` | 为已推送的精确 head 复用或创建 PR |
| `merge` | `delivery::MergeRequest` | 检查证据及实时 GitHub 门禁后合并精确 head |
| `knowledge_prepare` | `knowledge::PrepareReportRequest` | 准备隔离 SuperPOD 贡献工作树 |
| `knowledge_publish` | `knowledge::PublishReportRequest` | 提交、推送并创建审查后的 SuperPOD PR |
| `knowledge_refresh` | `knowledge::IndexRefreshRequest` | 更新/轮询精确 SuperPOD 索引目标 |

JSON 请求契约由 `crates/delivery/src/lib.rs` 和 `crates/knowledge/src/lib.rs` 的公开 Rust 类型定义。队列保存不可变哈希请求与持久结果，相同请求复用已有状态。索引任务可停留在 `waiting`，未知写入结果转为 `needs_reconciliation`。核对真实远程、工作树或安装状态后，可用 `automation reconcile-retry --id OPERATION_DIGEST` 重试；失败的发布门禁仍须满足。

发布操作要求 `outbox/evidence` 中的哈希证据文件：真实完整 Qualitygate 报告、将报告与代码/提示词/策略/SuperPOD 绑定的宿主快照回执、不同评估者的结果回执。安装快照还绑定包与二进制哈希，仅有布尔通过标记不够。当前上游默认分支绑定及技能清单哈希也必须具备，晋级/发布在执行前再次在线检查。旧回执不能授权新发布，不得伪造证据。

GitHub expected-head 前置条件不能原子冻结 base 分支；服务端竞态保证需要严格保持最新的分支保护或合并队列。安装将二进制放在技能目录，旧版放在技能发现范围之外；激活后冒烟验证，失败恢复旧版。后续变更会阻止旧回执覆盖新安装。GitHub 交付验证当前 CI、可合并性、必需评审和未解决讨论，不使用管理员绕过。

宿主也可显式调用 `install INPUT`、`rollback RECEIPT`、`pr INPUT`、`merge INPUT`、`knowledge-prepare INPUT`、`knowledge-publish INPUT`、`knowledge-refresh INPUT`。这些操作有实际副作用，控制器不会将任意模型输出转成发布权限。

## SuperPOD 发布与证据边界

启用 `require_latest = true` 时，各角色接收相同的有界 SuperPOD 入口包：`knowledge/index.md`、AI-SDLC 索引和证据方法文档。宿主读取已提交的普通 Git blob，记录提交、路径、完整源摘要、摘录摘要和裁剪状态。每段摘录最多 16 KiB，源文件须为非空 UTF-8 且不超过 1 MiB。准入时核对固定提交，Agent 从只读快照跟进相关链接；上下文投递不证明模型实际阅读或结论正确。

各角色在持久结果回执中提出有用补充、更正、反例及下一步实验，附已有 SuperPOD 目标路径和基线。宿主审查证据、准备隔离贡献、串行合并已审查更新，并刷新精确合并提交的索引。模型提案或任务成功不会自动发布知识。relay-memory 负责隔离任务连续性，SuperPOD 负责共享版本知识；没有入口包的历史回执保留原提示词与批次身份。

`knowledge-prepare` 在专用 SuperPOD 工作树的 `knowledge/software/ai-sdlc/` 下生成类型化、审查过的研究报告，集中保存来源 ID、引用、获取状态、校验和与不可变实验绑定。实测结论、外部主张、推断和未知项为不同类型。本地来源必须位于已有 `sources/` 归档并匹配校验和。素材获取及共享目录整理仍需明确研究操作，不自动下载或分类任意来源。

知识地图变更通过 relay-knowledge 命令生成。发布面向 `stevetdp/superpod` 的 PR，保持其可见性，不直接提交默认分支。索引就绪必须对应精确目标提交，已提交或排队不等于知识已刷新。代码与知识发布分别记录结果，跨仓库写入不具备原子性。

运行文件、`.lab/`、`.qualitygate/`、本地配置、数据库、日志和凭证均排除在 Git 外。公开前审查显式导出内容：扫描可发现部分明显泄露，不能决定任意文字和证据是否适合公开。系统提供执行、评估和交付机制，不预先声称提示词已改进或新能力已产生。

## 验证与缓存

```sh
bazel test --lockfile_mode=error //...
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
```

测试覆盖真实临时 Git 工作树、安装/回滚、进程取消、状态恢复、提示词门禁和 Bubblewrap 隔离。真实 workflow 协议测试通过 `LAB_WORKFLOW_BIN` 单独启用，其默认忽略状态不表示远程服务或模型已通过。正式交付还要求最终快照的完整 full Qualitygate。GitHub Actions 执行 Bazel 构建/测试和 Rust 格式/lint 检查。

构建任务（写任务或显式要求 `qualitygate-cli` 的任务）在空闲宿主预算窗口、领取尝试前准备私有依赖缓存。Cargo 只导入锁定的 crates.io 归档及公开稀疏索引项。Bazel 只导入冻结锁文件可达的哈希、已校验注册表和 Rust/Python 规则元数据，每个归档再次校验。不共享凭证、宿主锁文件、解压仓库或可写硬链接。

缺失缓存记录在 `build-cache-seed.json`，可能仍需网络。该标记只证明准备情况；新复制和复用文件均受 4,096 个文件、总计 2 GiB、单文件 512 MiB 限制。时限耗尽后保留进度，不领取任务。

尝试结束且无 worker/核对工作使用后，可移除其 `runs/<run-id>/cache` 与 `runs/<run-id>/home/.cargo`。准备报告、进程日志、回执、目标证据和工作树保留到满足留存策略，控制器不自动删除。真实缓存集成探测保留生产命名空间边界，要求离线 Clippy 和禁用仓库下载的 Bazel 构建/打包。完整控制器测试在可信宿主门禁运行，因为隔离回归会创建新命名空间，而 Agent 外层 `--disable-userns` 会拒绝这些操作。

## 持久群聊与事件观察

生产协作日志由独立存储服务持久化，其他服务通过 API 和事件流访问。数字人可参与多个公开或私有会话。私有受众创建后固定，改变成员范围需创建新会话；群标题、主题、置顶、归档及成员在工作区管理。数字人页面管理身份，系统设置管理服务与执行绑定。研究应用继续显示原有证据绑定提案板，保留冻结上下文、配额、撤权和实验权限，不重放进新收件箱。

```sh
bazel build -c opt //:agent-research-lab
bazel-bin/agent-research-lab --config local.toml serve --listen 127.0.0.1:8090
# 在另一个宿主终端获取私有浏览器登录链接：
bazel-bin/agent-research-lab --config local.toml serve-link
```

管理链接保存在 worker 挂载不可见的 `state_dir/dashboard/access.json`。首次打开建立 HttpOnly、SameSite 浏览器会话；此链接属于凭证，不得提交、放进 worker 提示词或公开。普通数字人 bearer 授权不能读取操作员面板、私有记忆、设置或其他会话。服务面向单一可信宿主，绑定回环地址；常见浏览器凭证目录也对执行器隐藏，同用户运行的任意宿主进程仍在信任范围内。

创建数字人或使用宿主管理的研究执行无需在界面申请个人接入凭证。需要从独立受信适配器访问底层消息 API 时，操作员可在 `POST /api/crystal/manage` 使用 `operation: "grant"` 签发短期授权：只返回一次、仅保存摘要、最长 24 小时有效。操作员变更还须携带精确同源 `Origin`、JSON Content-Type 和 `X-Crystal-Intent: manage-crystal`。

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

在**新的私有目录**中运行两种通信负载：

```sh
bazel-bin/agent-research-lab crystal-bench --state /private/new-mixed-run \
  --mode mixed --people 10000 --messages 10000 --rate 1000
bazel-bin/agent-research-lab crystal-bench --state /private/new-hot-run \
  --mode hot --people 10000 --messages 100 --rate 10
```

实验使用真实回环 TCP/SSE 和 SQLite 事务，分别报告发布确认与接收方投递的 P50/P95/P99、精确样本数、队列/重放/拒绝计数及载荷大小。不启动 10,000 个模型进程，也不覆盖浏览器渲染、持续 SLA、网络分区或断电。基准状态、凭证和原始回执留在 Git 外；汇总研究及绑定来源的最终测量进入 SuperPOD。

## 证据绑定的策略谱系

`evolution lineage REQUEST.json --output STATE/evolution/lineage.json` 基于既有 `PromptRegistry` 生成可重算回执。请求包含 `registry_file` 和 `study`；后者带有 `task_kind`、固定的 `policy_version` / `superpod_commit`、宿主冻结的 `expected` 快照绑定、开发观测 `observations`、待评估/扩展项及评估预算。研究观察应用的**系统设置 → 群体演化**投影已校验回执，不暴露提示词正文或试验输入；AI-IM 右侧版本演化展示当前群关联主题的版本与评估状态。

报告区分策略自身表现和后代汇总表现，对重组谱系共享后代去重，未完成工作也计入预算。验证/保留集观测、变化绑定和重复试验会被拒绝。这是受 HGM 启发的谱系诊断，不是 HGM Thompson sampling 调度器复现。候选激活继续受评估、保留集隔离、晋级、发布和安装门禁约束；策略分支不会创建新数字人身份或共享可变实验记忆。

聊天沿用数字人笑脸头像，群聊、成员列表与水晶球使用一致的身份标识。消息展示昵称、时间、气泡和可定位的引用回复；正文不会重复显示发言者自称，历史原文与研究证据保持完整。

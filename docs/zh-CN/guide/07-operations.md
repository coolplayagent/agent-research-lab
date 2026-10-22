# 07 部署、维护与诊断

[文档首页](../00-index.md) · [本目录](00-index.md) · [English](../../en/guide/07-operations.md)

## 服务部署

使用 `deploy/systemd/outcome-hub-cli.service` 用户服务模板前，按机器调整程序、工作目录和私有配置路径。构建不会自动安装或启用服务。同一状态只运行一个统一核心服务；独立存储、执行器和沙箱按[服务接入指南](03-services.md)部署。升级先停止旧服务，保留旧二进制与一致数据库备份，再安装已验证产物并重启。回退跨数据库版本的二进制时必须恢复匹配备份，不允许旧程序打开新数据库。

## 本地实验目标

控制器可准备可丢弃的本地目标，无需个人桌面或远程机器配置。Ubuntu 上的 `targets prepare` 将经过认证的 APT 包下载到 `.lab/tools/desktop`，无需 sudo，也不安装系统包。实际桌面使用 Xvfb、Openbox 和项目自有 GTK 输入程序。每个计算机研究任务拥有独立显示、认证 cookie 和输出目录；控制器先观察其唯一窗口，Agent 再通过已安装的 computer-use CLI 操作并对照真实 GTK 事件回执。

repo-sandbox 的 local provider 使用宿主工具，为每次执行创建真实 Bubblewrap 命名空间。持久会话记录工作区并报告 `ready`，不表示存在常驻容器。控制器使用私有注册表运行 `target install`、`dev up/exec/status/down`，并验证实际工作区写入。这些操作在宿主执行，因为部分 AppArmor 策略禁止 Agent 内嵌套用户命名空间。模型可读取保留的证据及私有注册表；需要 WSL/VM 语义时可使用对应目标。

```sh
bazel-bin/outcome-hub-cli --config local.toml targets prepare
bazel-bin/outcome-hub-cli --config local.toml targets verify --kind all
bazel-bin/outcome-hub-cli --config local.toml targets status
bazel-bin/outcome-hub-cli --config local.toml targets agent-check
bazel-bin/outcome-hub-cli --config local.toml doctor
bazel-bin/outcome-hub-cli --config local.toml doctor --probe-models
```

`targets verify` 要求支持 local provider 的 repo-sandbox 构建，旧运行时会明确失败。准备过程有时限且可恢复；缺失依赖产生可操作错误，不阻断无关研究。真实验证日志和截图保存在 `.lab/runs`。`targets agent-check` 经生产目标启动器调用已配置的评审模型，核对实际工具事件和 PNG；它消耗模型额度，证明的是传输链路。

控制器管理的 Codex worker 和模型探测显式使用 `approval_policy = "never"` 与 `sandbox_mode = "danger-full-access"`，每次尝试记录到 `agent-permissions.json`。控制器负责 8 Agent 上限，并禁用嵌套 Codex Agent。外层挂载/PID 隔离仍保护宿主凭证、其他尝试及评估保留集；操作系统权限错误需诊断目标或命名空间。

模型探测为有界真实调用，会消耗提供商额度。示例为研究配置 `gpt-6-astra`、实现配置 `gpt-6.1-sol`、评审配置 `gpt-6-sol`。可用性取决于账号，不可用时明确报错，不静默换模型。JSON 命令返回 `ok` 和 `result`，或返回错误并以非零状态退出。

## 观察、边界与长期服务

经认证的同源 SSE 推送变化并自动重连。Linux 文件系统通知触发 workflow/提案板投影和最多 16 个活动会话投影。节点脉冲表示新观察到的会话字节，连线活动表示新提案板记录；连线来源于实际发布或初始上下文投递。stdio-json 适配器投影公开 Codex 消息、命令、工具结果和轮次状态；其他 JSON bridge 可输出可选 `session.activity` 事件。推理字段、提示词和原始协议对象不展示。缺少源时间戳的事件保持无时间，界面另标明日志修改时间。

观察器使用控制器的 workflow 状态规则，读取已校验的提案板权限，不轮询 worker outbox。过期/撤销提案标为历史，不重新注入上下文。上下文注入不能证明采纳，workflow 成功不能证明研究质量，候选提交不能证明通过门禁或已安装。

采样最多扫描 8,192 个目录项，选择最近更新的 512 个 Job；每个 Job 上限 512 KiB、关联提案板上限 32 个，workflow 采样时限 20 秒。研究观察图最多展示 8 个 Agent，优先活动运行；全部选中任务仍可从会话列表访问。群聊显示最多 256 条提案板消息。每个会话页读取最多 256 KiB，投影最多 48 个公开事件，使用稳定字节偏移 ID。`before` 为排他的字节游标，向前翻页不丢失页边界处完整行。实时页随推送字节变化更新，尾部不完整行及超预算行省略，公开字段保持裁剪。

采样是顺序观察，不是原子数据库快照。失败时保留前次数据及错误时间。慢速宿主读取最多并发 16 项，异步流上限另为 20,100 个订阅；断连、撤权或停机结束流。关闭服务为网络排空及有界宿主工作预留 5 秒。

原有控制器在私有 `controller-status.json`、`seed-status.json` 报告心跳、活动任务和最新输入刷新状态。启动先初始化 workflow 存储，再恢复作业。连续模式刷新失败保持禁止准入，30 秒后重试并退避至 5 分钟，成功后恢复 15 分钟周期。界面分别报告刷新失败、心跳延迟和连接状态。systemd 模板重启每次最长 12 小时的进程；每日预算及暂停状态跨重启保留。

长期部署应将验证过的技能树与清单固定到私有 `tools/skills`，绑定控制器工具及可选私有 Codex profile。全局技能更新不得改变进行中实验的工具树。新版本需要新的源码/清单验证和新准入批次，不清除旧阻塞项或修改冻结证据。

服务仅绑定回环地址，使用 Host/Origin 检查、no-store 和同源内容策略。研究观察通过固定读取路由提供；管理写入使用独立的同源及 intent 契约。提案、公开 Agent 输出、工具摘要和阻塞记录均可能含研究信息，应作为私有数据。常见凭证字段会隐藏，但这不是通用秘密扫描器。原始日志、租约、任务记忆和任意文件下载不开放，不加载外部资源或遥测。

## 本地工作空间访问

启动后直接打开本地服务地址即可进入工作空间。`state_dir/dashboard/access.json` 只为 `link` 和 `serve-link` 记录普通 URL，不包含管理凭据。旧管理凭据文件保持原样，但不再读取。数字人的 bearer 授权仍只适用于其范围内的 API，不能获取宿主管理权限。本服务面向单一可信宿主；能访问其本机回环监听的进程或浏览器都处于宿主信任边界内。

创建数字人或使用宿主管理的研究执行无需在界面申请个人接入凭证。需要从独立受信适配器访问底层消息 API 时，操作员可在 `POST /api/v1/messaging/manage` 使用 `operation: "grant"` 签发短期授权：只返回一次、仅保存摘要、最长 24 小时有效。操作员变更还须携带精确同源 `Origin`、JSON Content-Type 和 `X-Crystal-Intent: manage-crystal`。

## 通信基准

在**新的私有目录**中运行两种通信负载：

```sh
bazel-bin/outcome-hub-cli crystal-bench --state /private/new-mixed-run \
  --mode mixed --people 10000 --messages 10000 --rate 1000
bazel-bin/outcome-hub-cli crystal-bench --state /private/new-hot-run \
  --mode hot --people 10000 --messages 100 --rate 10
```

实验使用真实回环 TCP/SSE 和 SQLite 事务，分别报告发布确认与接收方投递的 P50/P95/P99、精确样本数、队列/重放/拒绝计数及载荷大小。不启动 10,000 个模型进程，也不覆盖浏览器渲染、持续 SLA、网络分区或断电。基准状态、凭证和原始回执留在 Git 外；汇总研究及绑定来源的最终测量进入 SuperPOD。

## 网络访问

`serve` 的 `--listen` 只接受回环地址；通配地址和网卡地址会在启动时被拒绝。浏览器请求仍检查 Host 和 Origin，管理写入仍需对应 intent 请求头。若需从其他计算机访问，应另行提供带访问控制的入口，不要把免登录服务直接绑定到网卡。

## 存储迁移

存储服务仅接受 schema 10。更换存储部署前，使用 `outcome-hub-migrate` 处理离线副本；运行中的服务不会升级数据库。schema 10 删除版本节点、评估及迭代存储。核心与存储二进制须配套部署。

升级删除迭代生成的所有轮次目标、已移除软件演进场景的目标，以及其任务结果和成果记录；对应系统消息清空正文与事件载荷，保留消息身份、回复引用和游标。独立目标和普通聊天执行字节保留校验。独立成果若引用待删除证据，会阻止迁移，须先明确处理关联。已有迁移记录只在准备好的副本中替换，原始记录摘要仍包含在来源清单中。

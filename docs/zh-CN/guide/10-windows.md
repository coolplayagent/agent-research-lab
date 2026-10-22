# 10 Windows 原生部署

[使用文档](00-index.md) · [English](../../en/guide/10-windows.md)

从同一个 [GitHub Release](https://github.com/coolplayagent/agent-research-lab/releases) 下载 `outcome-hub-windows-x86_64.zip` 和 `SHA256SUMS`。先在 PowerShell 中校验压缩包，再解压：

```powershell
$expected = (Get-Content .\SHA256SUMS | Where-Object { $_ -match 'outcome-hub-windows-x86_64\.zip$' }).Split(' ')[0]
$actual = (Get-FileHash .\outcome-hub-windows-x86_64.zip -Algorithm SHA256).Hash.ToLowerInvariant()
if ($actual -ne $expected) { throw '压缩包 SHA-256 不匹配' }
Expand-Archive .\outcome-hub-windows-x86_64.zip -DestinationPath .\outcome-hub-release
```

在 `outcome-hub-release\outcome-hub` 打开 PowerShell，启动原生服务：

```powershell
.\outcome-hub.exe serve --listen 127.0.0.1:4877 --max-seconds 43200
```

在这台计算机的浏览器中直接打开 `http://127.0.0.1:4877/`，无需管理凭据。默认状态目录是 `%LOCALAPPDATA%\OutcomeHub\state`；使用绝对路径 `--state-dir` 可更换目录。程序为状态设置私有访问控制列表。升级前备份状态目录，恢复备份前停止服务。

当前源码构建的完整包包括独立存储、执行器、沙箱、Agent bridge 和知识库服务。新工作空间自动管理服务地址：存储随核心启动，执行与知识服务按需准备。“设置 → 本机环境”显示状态与“一键准备”；外部地址仅在高级部署中配置。使用 Git 知识库需已安装 Git for Windows；目录知识库不需要 Git。已有部署保留原数据位置与服务选择。

从源码构建需安装仓库固定版本的 Rust 工具链及 Bazelisk，然后运行 `bazel build --lockfile_mode=error //:windows-package`。CI 在 `windows-latest` 上构建、解压并实际启动包内的可执行文件。

## v0.2.1 中的原生 Agent 绑定

[v0.2.1](https://github.com/coolplayagent/agent-research-lab/releases/tag/v0.2.1) 新增 `outcome-hub-services.exe` 和 `outcome-hub-agent.exe`，请将它们与 `outcome-hub.exe` 放在同一目录。本版修复了 Windows 规范化路径处理导致绑定出现 OS error 1 的问题。Windows 自动化测试覆盖核心启动、进程隔离、四种 Agent 的确定性协议测试程序和已安装 Node 的启动处理。维护者于 2026-10-21 确认，v0.2.1 的四种 Agent 已完成 Windows 登录与真实模型验收。此项为维护者确认，私有真实运行报告不进入仓库；自动化测试程序仅证明其覆盖范围内的协议与隔离行为。

进入“设置 → Agent 与执行器绑定”，选择数字人以及 Codex、Claude Code、DeepSeek Harness 或 Pi，点击“验证并绑定数字人”。程序自动查找已安装的 Agent，不会安装第三方 CLI。支持独立 `codex.exe`；自动查找失败时，可填写完整程序路径。模型留空时沿用本地选择；DeepSeek Harness 需要明确的已有模型路由，或手动填写 `provider/model`。

绑定前会执行一次小型模型任务，可能产生费用。界面依次显示查找程序、运行环境、准备配置、执行验证、保存绑定。可取消或刷新页面恢复进度，最长等待 120 秒。失败时保留原绑定；验证期间设置发生变化时，需要刷新后重新验证。已有绑定在重新验证前显示“已配置，尚未验证”。

使用 Windows 10/11 x64 和本地 NTFS 状态目录。遇到系统错误码（例如 OS error 1）时，界面会显示失败阶段和诊断编号。按阶段检查状态磁盘权限、文件系统支持、Agent 登录和模型设置；详细记录位于状态目录的 private 子目录。不要公开凭据或私有诊断文件。

绑定诊断文件为 `private/binding-<request-id>.diagnostic.json`。原生命令失败时，记录可能引用 `sandbox-...` 编号，对应的限量错误输出位于 `private/managed-sandbox/<编号>.diagnostic.json`。这些文件保留在本机，可能含有敏感 Agent 输出。

在四种 CLI 均已安装并登录的 Windows 机器上，从源码目录运行真实验收：

```powershell
./verification/windows-binding-smoke.ps1 -Binary C:\OutcomeHub\outcome-hub.exe -Live -Report $env:TEMP\outcome-hub-live.json
```

可用 `-Programs @{codex='C:\Tools\codex.exe'}` 指定程序，或用 `-Models @{hds='provider/model'}` 覆盖模型。报告作为私有证据保留，不要提交 Git。CI 的确定性测试程序用于验证通信、隔离与绑定流程，不代表四种真实 Agent 已通过模型验收。

# 10 Windows 原生部署

[使用文档](00-index.md) · [English](../../en/guide/10-windows.md)

从同一个 [GitHub Release](https://github.com/coolplayagent/agent-research-lab/releases) 下载 `outcome-hub-windows-x86_64.zip` 和 `SHA256SUMS`。先在 PowerShell 中校验压缩包，再解压：

```powershell
$expected = (Get-Content .\SHA256SUMS | Where-Object { $_ -match 'outcome-hub-windows-x86_64\.zip$' }).Split(' ')[0]
$actual = (Get-FileHash .\outcome-hub-windows-x86_64.zip -Algorithm SHA256).Hash.ToLowerInvariant()
if ($actual -ne $expected) { throw '压缩包 SHA-256 不匹配' }
Expand-Archive .\outcome-hub-windows-x86_64.zip -DestinationPath .\outcome-hub-release
```

在 `outcome-hub-release\outcome-hub` 打开两个 PowerShell 窗口，分别启动原生服务并获取私有浏览器链接：

```powershell
.\outcome-hub.exe serve --listen 127.0.0.1:4877 --max-seconds 43200
.\outcome-hub.exe link
```

默认状态目录是 `%LOCALAPPDATA%\OutcomeHub\state`。如需更换目录，两个命令都使用相同的绝对路径 `--state-dir`。程序为状态设置私有访问控制列表。访问链接属于凭证；升级前备份状态目录，恢复备份前停止服务。

此 Windows 原生包提供核心协作及独立的嵌入式本地存储。外部 Agent 执行、沙箱、研究与知识库服务不在这个包内，Windows 默认关闭这些服务；原生服务传输适配器接入之前，已配置的外部服务套接字仍无法连接。配置完成不等于任务已执行成功。

从源码构建需安装仓库固定版本的 Rust 工具链及 Bazelisk，然后运行 `bazel build --lockfile_mode=error //:windows-package`。CI 在 `windows-latest` 上构建、解压并实际启动包内的可执行文件。

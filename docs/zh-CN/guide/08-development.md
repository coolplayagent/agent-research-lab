# 08 开发、测试与验证

[文档首页](../00-index.md) · [本目录](00-index.md) · [English](../../en/guide/08-development.md)

## 构建入口

每个 crate 在 Cargo 与 Bazel 中声明依赖，依赖版本来自工作区锁文件。Bazel 是构建、测试和打包入口；`//:tests` 汇总测试，`//:agent-research-lab` 生成 `bazel-bin/agent-research-lab`，`//:package` 生成包含双语文档的发布包。可用 `bazel test //crates/communication:tests` 定位单模块回归。

```sh
bazel build --lockfile_mode=error //:agent-research-lab //:package
```

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

## 前端生成文件

Node 22.22.1 与 TypeScript 5.9.3 由 Bazel 固定。提交的 `dist/` 用于 Rust 嵌入和 Cargo 检查；Bazel 校验它们与 TypeScript 编译结果一致。修改后重新生成：

```sh
bazel build //crates/im-service/web:compiled //app/src/dashboard/web:compiled
install -m 644 bazel-bin/crates/im-service/web/compiled/*.js crates/im-service/web/dist/
install -m 644 bazel-bin/app/src/dashboard/web/compiled/*.js app/src/dashboard/web/dist/
bazel test --lockfile_mode=error //...
```

共享状态从 Rust `contracts` 生成，须验证生成结果无漂移。运行仓库完整测试和完整 Qualitygate，保留最终快照与策略摘要。语言资源、参数完整性、API 兼容性和服务端/展示层边界都有回归；文本检查不能代替架构及状态机评审。

## 界面冒烟验证

使用专用测试群和数字人，检查创建、成员选择、搜索及清除、多行 Unicode 消息、回复、目标创建与主持人验收。SSE 更新时待发送按钮仍应锁定，重试保持请求 ID。检查桌面、平板和手机宽度、键盘导航、模态焦点、溢出，以及研究设置和观察视图。原始截图和运行状态留在 Git 外。

## 文档维护

中英文编号和文件名一一对应，同次更新两种语言与导航链接。设计说明职责与约束，指南说明操作与结果。示例 JSON、协议值、路径和命令不翻译。文档双语不表示产品界面已支持英文。

## 原生批次集成测试

真实 owner 回归为显式启用的 Bazel 测试。将 `LAB_WORKFLOW_BIN` 指向被测 CLI 0.2.0 运行时，对 `//crates/runtime:tests` 使用 `--test_arg=batch::tests --test_arg=--ignored`。独立 `//app:enqueue_batch_cli_tests` 还需 `LAB_CLI_BIN`，通过隔离的 GitHub/fetch 获取夹具验证真实 CLI 与新鲜度收集器。这些测试仅创建自有临时存储，不运行模型。

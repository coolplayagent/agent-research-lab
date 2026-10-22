# OutcomeHub legacy 退役与离线迁移

## 正式入口

仅分发 `outcome-hub`、`outcome-hub-cli`、`outcome-hub-storage`、`outcome-hub-services`、`outcome-hub-codex-adapter` 和 `outcome-hub-knowledge`。旧 `ai-im*`、`agent-research-lab` 可执行入口与旧 Bazel 标签已删除。

HTTP 使用 `/api/v1/collaboration/*`、`/api/v1/messaging/*`、`/api/v1/agent/outcomes`；协调操作使用 `/api/v1/collaboration/coordination`，控制台使用 `/apps/console/`。旧 `/api/im/*`、`/api/crystal/*`、`/apps/research/` 不再提供服务。管理与 Agent 的权限边界保持独立。

现有 `.ai-im` 默认路径、认证 Cookie、协议头、状态序列化值和外部仓库标识保留，不因源码清理自动改名。当前 v1 配置的可选字段及冻结证据格式继续有效；移除的是运行时的旧格式升级和补写流程。

## 离线工具

`outcome-hub-migrate` 是单独分发的 Rust 工具，核心服务不依赖它。它支持数据库 schema 1–9、旧 `dashboard/personas.json`、任务身份关联以及已知的旧完成日志。未知版本、损坏数据、未知完成状态、无法绑定冻结任务或缺失必要回执均拒绝转换。

```sh
bazel build --lockfile_mode=error //:migration-package
bazel run //:outcome-hub-migrate -- inspect --source /ABS/old-state
bazel run //:outcome-hub-migrate -- convert --source /ABS/old-state --destination /ABS/prepared-state
bazel run //:outcome-hub-migrate -- verify --source /ABS/old-state --destination /ABS/prepared-state
```

先停止使用源目录的核心服务、存储服务、控制器及任务进程，保存旧二进制和一致备份。源可以是包含 `messages.sqlite` 的存储目录，也可以是包含存储子目录、人员目录和任务记录的应用状态目录。工具检查现有锁和任务进程；操作者必须保证迁移期间服务不重启。

目标必须不存在且位于源目录之外。工具只转换私有临时副本，保留源文件；校验完成后以禁止覆盖的原子重命名发布。符号链接、特殊文件和超出边界的输入会失败。限制为 100,000 个目录项、64 层目录、单文件 8 GiB、总文件 32 GiB；大型存储应按独立服务状态目录处理。失败时不会发布半成品；中断残留的 `.outcome-hub-migrate-*` 私有目录不得作为服务目录启动。

迁移清单记录工具版本、源与目标的逐文件摘要、整体摘要以及转换项。数据库转换校验原有列的全部行内容，并检查完整性和引用。清单用于本地迁移一致性核对，不是研究验收或发布凭证。验证必须在首次启动目标服务之前执行；服务启动后会正常改变目标数据。

冻结任务、回执和实验来源／提示词／策略／知识版本证据不重写。被转换的人员目录和日志原文保存在目标的 `migration-originals/`。历史副作用缺少可信完成记录时，内存回写保持 `Unknown`，后续任务准入保持 `Blocked`，不自动重放。需要核对原始权威证据才能恢复后续操作，迁移不会补造成功回执或提高研究评估结果。

## 切换与回退

1. 完成停机、备份、转换和离线校验。
2. 显式配置新服务的数据目录和端点；生产存储只允许一个所有者。
3. 更新 systemd 模板中的执行文件并重启。模板不会自动选择迁移目录。
4. 检查服务健康、身份、消息、游标及任务关联。旧工作目录和冻结机器路径不自动改写；需要新执行环境时创建新任务，不能绕过原有策略摘要检查。
5. 回退时停止新服务，使用旧二进制和原目录／一致备份。不得让旧程序读取新运行后改变的数据，也不执行反向迁移。

主存储仅创建和接受 schema 10，旧版本在修改数据库之前拒绝。研究控制器与控制台在启动前拒绝旧人员目录和不完整完成日志。

## 内容归属

仓库不再包含 `skills/`、`research/`、`IM.md` 或根级 `architecture/`。技能从外部分发源安装并记录版本证据；研究知识由外部知识仓库管理。新研究输入要求绑定仓库中已提交的 `knowledge/software/ai-sdlc/insights.md`；文件不存在时明确失败，不削弱 SuperPOD pins 或门禁。本次代码变更不写入外部知识库。

可执行架构边界与 wire 回归检查位于 `verification/`，继续参与 `bazel test //...`。历史数据 fixture 保留在离线迁移工具的测试目录，当前协议 fixture 保留在验证目录。

## 删除版本演化与迭代数据

schema 10 离线升级会删除版本节点、父边、评估、迭代记录和关联目标、任务结果与成果。关联系统消息仅保留身份及引用位置，清空正文与事件载荷；普通聊天和独立目标保持完整。新版本仅接受 schema 10；回退必须同时恢复旧二进制与离线备份。存在引用待删证据的独立成果时，迁移会停止。已有迁移记录的旧数据库支持再次升级，来源文件始终保持不变。

## 成果协作迁移

schema 10 删除协作主题、研究对象、成果路径，以及主题所属会话关联的目标、成果和专用证据；普通聊天与无关目标保留，外部知识仓库不删除。先停止核心与存储服务，再对离线存储执行 convert，审核报告并切换配套二进制。内置知识服务升级为成果协议 v2，配置自动生成私有备份并递增 revision；回退须同步恢复数据库、配置和旧二进制。

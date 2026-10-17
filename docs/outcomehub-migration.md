# OutcomeHub 命名与接口迁移

## 应用和目录

`app/` 已合并进 `apps/cli/`，不保留重复入口目录或软链接。

| 原名称 | 当前名称 |
| --- | --- |
| `apps/ai-im` | `apps/outcome-hub` |
| `apps/research-cli` | `apps/cli` |
| `apps/research-console` | `apps/console` |
| `crystal` | `collaboration-store` |
| `im-storage` | `collaboration-storage-service` |
| `im-client`、`im-host`、`im-service` | `collaboration-client`、`collaboration-host`、`collaboration-api` |
| `communication`、`multi-agent` | `collaboration-proposals`、`agent-teams` |
| `collaboration-experiment` | `collaboration-evaluation` |
| `config`、`task`、`runtime` | `execution-config`、`execution-model`、`execution-runtime` |
| `budget`、`inputs`、`targets` | `execution-budget`、`execution-inputs`、`execution-targets` |
| `workflow`、`freshness` | `workflow-adapter`、`source-freshness` |
| `automation`、`delivery` | `delivery-outbox`、`delivery-adapters` |
| `evolution`、`evolution-cli` | `prompt-evolution`、`prompt-evolution-cli` |
| `storage` | `filesystem-store` |
| `knowledge` | 通用 `knowledge-*` 模块及显式报告流程 `knowledge-publication` |

前端移入 `web/outcome-hub`、`web/console`；共享契约与国际化设施位于 `web/shared`。systemd 模板集中到 `deploy/systemd`。Cargo 包使用 `outcome-hub-` 前缀，内部库依赖按新位置更新；部分通用依赖别名及旧类型重导出用于保持已有代码调用兼容。

## 命令与构建

| 旧命令（保留） | 规范命令 |
| --- | --- |
| `ai-im` | `outcome-hub` |
| `agent-research-lab` | `outcome-hub-cli` |
| `ai-im-storage` | `outcome-hub-storage` |
| `ai-im-services` | `outcome-hub-services` |
| `ai-im-codex-adapter` | `outcome-hub-codex-adapter` |
| `ai-im-outcome-service` | `outcome-hub-outcomes` |
| 新增 | `outcome-hub-knowledge` |

根级 `//:agent-research-lab`、`//:ai-im`、`//:im-package` 保留兼容；新项目使用 `//:outcome-hub`、`//:outcome-hub-cli`、`//:outcome-hub-package`。旧内部目录对应的 Bazel 标签需按表迁移。

## HTTP

| 旧入口 | 规范入口 |
| --- | --- |
| `/api/im/*` | `/api/v1/collaboration/*` |
| `/api/im/research` | `/api/v1/collaboration/coordination` |
| `/api/crystal/*` | `/api/v1/messaging/*` |
| `/api/crystal/outcomes` | `/api/v1/agent/outcomes` |
| `/apps/research/` | `/apps/console/` |
| 新增知识库管理 | `/api/v1/knowledge` |

别名复用原处理器与授权规则，不通过 HTTP 重定向重放 POST 或流式请求。管理端成果操作与 Agent 成果提交仍是不同权限。既有认证 Cookie、请求意图头和序列化状态值保留兼容。

## 知识库迁移

1. 保留旧服务、数据库和发布回执的备份及版本记录。
2. 启动新的 `outcome-hub-knowledge` 服务，使用独立状态目录。
3. 将旧配置的仓库位置和分支转换为 `knowledge set` 使用的 Git 目标配置，显式指定 `content_root`、权限与可选远端。
4. 在服务设置中注册新服务，检查目标连接，再为下一次成果发布选择该目标。
5. 已发出的旧请求继续由原服务查询回执；不要把同一个请求 ID 转交给不同目标执行。

普通目标不需要 SuperPOD 配置。已有报告工作流须增加明确的 `layout`，包括报告目录、允许修改的导航文件和生成目录、主题、仓库及预期 origin；旧准备请求和待发布报告需在明确审核后补充布局配置；不要自动重写尚未完成的发布操作。旧报告证据字段 `superpod_commit`、`superpod_root` 仍可反序列化，新输入也接受 `knowledge_revision`、`knowledge_root`。历史证据摘要不重算。

## 数据与服务

本次源码更名不搬迁 `.ai-im`、既有数据库或服务状态。新旧 systemd 模板是替代入口，切换时先停止旧实例，沿用明确准备的数据目录；不要同时启动两个存储所有者。生产存储端点变更仍要求显式重启。

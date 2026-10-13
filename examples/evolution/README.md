# 策略演化请求示例

[English](README.en.md) · [实验使用指南](../../docs/zh-CN/guide/05-experiments.md)

本目录 JSON 是合成夹具，不是实测研究结果；`evolution_cli` 测试使用真实 Rust 请求类型验证往返序列化。

宿主适配器接收动作、输入 JSON 和可选输出文件。JSON 内路径相对输入文件，输出路径相对当前 shell；父目录必须已存在。

| 动作 | 输入 | 输出行为 |
| --- | --- | --- |
| `prompt-register` | `PromptRegistration` | 必须指定注册表；校验既有不可变版本后追加，相同内容幂等 |
| `prompt-portfolio` | `PortfolioValidation` | 校验版本及 1 稳定 / 2 专门 / 2 探索限制，不激活策略 |
| `evaluate` | `PromotionRequest` | 计算晋级门禁，可选输出完整私有评估回执 |
| `novelty` | `NoveltyRequest` | 计算保留集迁移和消融门禁，同样可输出私有回执 |
| `exploration` | `ExplorationUpdate` | 必须指定原始 `ExplorationBudget` 状态，仅依可重算评估回执更新 |
| `rule-proposal` | `RuleProposalRequest` | 生成隔离提案，不采纳正式策略 |
| `promote` | `StrategyPromotionRequest` | 必须指定 selections 文件，重算已批准回执，核对当前上下文和谱系后选择未来任务候选 |
| `holdout-validate` | `HoldoutIsolation` | 检查规范路径分离与符号链接泄露，不证明 OS 隔离 |

复制门禁示例及 `context.json` 到宿主私有目录，将全部夹具 ID 和测量替换为真实证据。布局要求同级 `holdout`、`optimizer`、`superpod` 和 `memory` 目录；正式部署的保留集位于 `state_dir/private`，由进程沙箱隐藏。门禁输入、预期上下文、回执和预算不能放进优化器工作区或共享知识/记忆。

宿主单独固定 `context.json`。每次观测必须匹配预期基线/候选绑定，未知字段（包括传入 `approved`）报错。回执保留上下文和观测以重算决定，缺少证据失败。

新评估还需要捕获的 `upstream` 仓库、默认分支和提交，以及带宿主路径/SHA-256 的 `skills_manifest`；基线源码必须等于捕获的默认提交。把完整验证过的清单复制到宿主证据区，替换示例路径/哈希为真实树、运行时及发行身份，生产清单含七工具。不要指向可变全局清单。

v2 回执把上游与技能身份纳入证据摘要。晋级或探索预算更新前重新核对 GitHub 默认分支、最新技能发行及安装字节；分支推进、新发行、资产变更或断网均要求新评估，两份相同旧文件不能批准候选。正在运行的固定配对试验保持原输入；未绑定的历史回执仍可读，但不能授权新门禁。

先注册 `prompt-register.json`，再将 `prompt-portfolio.json` 的稳定版本占位替换为返回 SHA-256。派生候选在 `definition.parent_versions` 填父版本，修改内容和理由。`resolve_registered_prompt(registry_file, version, role)` 解析显式版本；注册本身不选择执行提示词。

独立评估成功后，`promote.json` 为角色选择确切候选。运行注册表为 `state_dir/evolution/prompts.json`，晋级输出为 `state_dir/evolution/selections.json`。候选须源自评估基线，首次晋级后基线须仍为当前选择。提示词、源码、模型/工具/记忆配置、正式策略或预期上下文变化不能复用批准。重复同一晋级幂等，每角色最多保留五个旧稳定版本，不提供自动规则采纳。

新任务入队调用 `resolve_selection(state_dir, role)` 并冻结版本；旧任务不变。没有选择时用初始角色模板，损坏或未知选择报错，不静默回退。

运行器从 `state_dir/evolution/exploration.json` 读取原始预算。探索按证据摘要去重，不按显示标签；改标签或观测顺序不能重复计数。替换状态前校验完整批次，新颖性证据本身不构成正式提示词晋级。

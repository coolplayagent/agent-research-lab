# 外部知识库接入

OutcomeHub 支持多个 Git 仓库和目录知识库。SuperPOD 是其中一种用户选择的目标实例，不需要专用模块。

## 1. 启动知识库服务

```bash
bazel build //crates/knowledge-service:outcome-hub-knowledge //:outcome-hub-cli
bazel run //crates/knowledge-service:outcome-hub-knowledge -- \
  --id knowledge \
  --state-dir /absolute/private/knowledge-service \
  --socket /absolute/private/knowledge.sock \
  --max-seconds 43200
```

服务状态与知识库正文必须分离。Socket 使用本机服务权限；CLI 操作者需要该 Socket 的访问权限。服务以有界生命周期运行，停止前进入排空阶段。

## 2. 配置 Git 知识库

准备一个服务可访问的本地 Git 仓库，并创建尚未被任何工作区检出的发布分支：

```bash
git -C /srv/knowledge/team branch outcomes main
```

把以下配置保存为仓外文件 `git-target.json`：

```json
{
  "kind": "git",
  "path": "/srv/knowledge/team",
  "content_root": "knowledge",
  "enabled": true,
  "access": "read_write",
  "reference": "refs/heads/outcomes",
  "remote": null,
  "credential_env": []
}
```

```bash
bazel run //:outcome-hub-cli -- knowledge --endpoint /absolute/private/knowledge.sock set team git-target.json
bazel run //:outcome-hub-cli -- knowledge --endpoint /absolute/private/knowledge.sock probe team
```

`remote: null` 仅更新配置的本地分支。填写已有远端名称，例如 `origin`，表示发布时还需推送；不会自动推送其他目标。远端冲突不会通过强制覆盖解决。凭据只存于服务环境中，`credential_env` 填变量名，不填写密码、Token 或带凭据的 URL。

读取保留文档内容及空白；Git 可指定提交读取历史版本。发布的标准成果文档位于 `<content_root>/accepted-outcomes/<outcome-id>.json`，同一树中记录发布身份。

## 3. 配置目录知识库

预先创建内容目录 `/srv/knowledge/notes/documents`，保存以下配置为 `directory-target.json`：

```json
{
  "kind": "directory",
  "path": "/srv/knowledge/notes",
  "content_root": "documents",
  "enabled": true,
  "access": "read_write",
  "reference": null,
  "remote": null,
  "credential_env": []
}
```

```bash
bazel run //:outcome-hub-cli -- knowledge --endpoint /absolute/private/knowledge.sock set notes directory-target.json
bazel run //:outcome-hub-cli -- knowledge --endpoint /absolute/private/knowledge.sock list
bazel run //:outcome-hub-cli -- knowledge --endpoint /absolute/private/knowledge.sock read notes guide.md
```

目录基线是受限内容树的摘要，不提供历史文件重建。目录必须由服务账户可访问，不接受路径穿越、符号链接及特殊文件。首版限制为 4096 个条目、32 层、每个文档 1 MiB、总内容 32 MiB；用于成果知识文档，不用于原始大数据运行目录。内部 `.outcomehub` 保存锁和发布回执，不应手动修改。

## 4. 使用 Console

1. 在服务设置中注册类型为“知识库”的成果服务，填写服务标识、Socket 和协议版本 `1`。
2. 在“外部知识库”面板加载目标，添加或编辑 Git／目录配置、访问权限、启用状态。
3. 检查连接。连接正常只表示目标可访问，不能作为成果已发布的证据。
4. 可为群组或目标设置默认知识库；默认选择顺序为本次指定、目标、群组、主题。打开已验收成果，选择知识库服务和目标，预览发布前后内容，再确认发布。
5. 查看发布状态、文档引用和版本。结果未知时使用回执核对，不盲目重发。

CLI 和 Console 使用相同的知识库管理协议与配置修订检查；Console 由已认证操作者通过 `/api/v1/knowledge` 调用。配置变更会使旧预览失效，需要重新预览。

CLI 也支持 `save`、`enable <name> --enabled true|false`、`remove`、`preview`、`publish` 和 `receipt`。`publish` 接收主持端生成并明确审核过的 `Write` 协议记录；日常成果验收及发布建议使用工作区流程，确保当前验收记录和成果版本一致。

## 5. 接入 SuperPOD 与自定义索引

将 Git 配置的 `path` 指向用户准备的 SuperPOD 仓库，填写其内容根目录、发布分支及可选远端即可。也可以将其目录配置为只读知识来源。目标名称由用户决定，例如 `superpod`。

已有报告流程使用 `knowledge-prepare` / `knowledge-publish` 命令及通用 `knowledge-publication` 模块。需要显式提供 `layout`：`report_directory`、`navigation_files`、`generated_roots`、`topic`、`repository`、`expected_origin`，以及既有的索引 CLI 路径。索引变更纳入审核清单和发布校验，不自动扩大可发布文件范围。它是可选的报告与索引适配流程，普通 Git／目录知识库不要求安装索引工具。

原有研究场景的 SuperPOD 基线和发布证据门禁仍保留；切换目标需要显式配置及新的审核记录。

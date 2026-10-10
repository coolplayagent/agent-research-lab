import { workspaceState } from "./state.js";
import { $, el } from "./ui.js";
export function renderExecutorDirectory(): any {
  const root = $("executor-directory");
  root.replaceChildren();
  const labels = {
    structured_result: "结构化结果",
    read_workspace: "读取工作区",
    write_workspace: "修改工作区",
    tool_execution: "工具与协作",
    desktop: "桌面操作",
  };
  for (const backend of workspaceState.executionCatalog.backends) {
    const card = el("article", undefined, "memory-event");
    card.append(
      el("h3", backend.id),
      el("p", backend.kind === "codex" ? "Codex 执行器" : "stdio JSON 执行器"),
    );
    card.append(
      el(
        "p",
        Object.entries(backend.capabilities || {})
          .filter(([, enabled]: any) => enabled)
          .map(([key]: any) => labels[key] || key)
          .join(" · "),
      ),
    );
    card.append(el("p", `模型：${(backend.models || []).join(" · ")}`));
    card.append(
      el("small", "服务已配置 · 能力为配置声明；任务仍需通过权限检查"),
    );
    root.append(card);
  }
  root.append(
    el(
      "p",
      `可选模型：${workspaceState.executionCatalog.models.join(" · ") || "尚未加载"}`,
    ),
  );
}
export function editPersonExecution(): any {
  const person = workspaceState.peopleIndex.get(workspaceState.personFocus);
  if (!person) return;
  workspaceState.executionEditor = person;
  for (const [field, options, value] of [
    [
      "backend",
      workspaceState.executionCatalog.backends.map((b?: any) => b.id),
      person.execution?.backend,
    ],
    ["model", workspaceState.executionCatalog.models, person.execution?.model],
  ]) {
    const select = $("person-" + field);
    select.replaceChildren();
    const inherited = el("option", "按任务默认配置");
    inherited.value = "";
    select.append(inherited);
    for (const id of new Set<any>([...options, ...(value ? [value] : [])])) {
      const option = el(
        "option",
        options.includes(id) ? id : `${id}（当前未配置）`,
      );
      option.value = id;
      select.append(option);
    }
    select.value = value || "";
  }
  $("person-execution-description").textContent =
    "专长描述思考方式，任务角色决定本次职责与权限。执行技术是可替换的工具；固定成员可以承担不同任务角色。";
  $("person-execution-status").textContent = "";
}
export function updateExecutionModels(): any {
  const backend = workspaceState.executionCatalog.backends.find(
    (b?: any) => b.id === $("person-backend").value,
  );
  const models = backend?.models || workspaceState.executionCatalog.models;
  for (const option of $("person-model").options)
    option.disabled = Boolean(option.value && !models.includes(option.value));
  if ($("person-model").selectedOptions[0]?.disabled)
    $("person-model").value = "";
}

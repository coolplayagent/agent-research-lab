import { t } from "./i18n.js";
import { workspaceState } from "./state.js";
import { $, el } from "./ui.js";
export function renderExecutorDirectory() {
    const root = $("executor-directory");
    root.replaceChildren();
    const labels = {
        structured_result: t("executors.0aa3cde470"),
        read_workspace: t("executors.0a7e537b79"),
        write_workspace: t("executors.a3c2910666"),
        tool_execution: t("executors.29b410e92b"),
        desktop: t("executors.46d820f7cd"),
    };
    for (const backend of workspaceState.executionCatalog.backends) {
        const card = el("article", undefined, "memory-event");
        card.append(el("h3", backend.id), el("p", backend.kind === "codex"
            ? t("executors.16861ed351")
            : t("executors.7c2a2a8e89")));
        card.append(el("p", Object.entries(backend.capabilities || {})
            .filter(([, enabled]) => enabled)
            .map(([key]) => labels[key] || key)
            .join(" · ")));
        card.append(el("p", t("executors.c971dbc596", { p0: (backend.models || []).join(" · ") })));
        card.append(el("small", t("executors.1d66d49e28")));
        root.append(card);
    }
    root.append(el("p", t("executors.ddb7fd26c2", {
        p0: workspaceState.executionCatalog.models.join(" · ") ||
            t("executors.63c1fe8533"),
    })));
}
export function editPersonExecution() {
    const person = workspaceState.peopleIndex.get(workspaceState.personFocus);
    if (!person)
        return;
    workspaceState.executionEditor = person;
    for (const [field, options, value] of [
        [
            "backend",
            workspaceState.executionCatalog.backends.map((b) => b.id),
            person.execution?.backend,
        ],
        ["model", workspaceState.executionCatalog.models, person.execution?.model],
    ]) {
        const select = $("person-" + field);
        select.replaceChildren();
        const inherited = el("option", t("executors.64bfaa1be0"));
        inherited.value = "";
        select.append(inherited);
        for (const id of new Set([...options, ...(value ? [value] : [])])) {
            const option = el("option", options.includes(id) ? id : t("executors.de44191531", { p0: id }));
            option.value = id;
            select.append(option);
        }
        select.value = value || "";
    }
    $("person-execution-description").textContent = t("executors.ecee08a207");
    $("person-execution-status").textContent = "";
}
export function updateExecutionModels() {
    const backend = workspaceState.executionCatalog.backends.find((b) => b.id === $("person-backend").value);
    const models = backend?.models || workspaceState.executionCatalog.models;
    for (const option of $("person-model").options)
        option.disabled = Boolean(option.value && !models.includes(option.value));
    if ($("person-model").selectedOptions[0]?.disabled)
        $("person-model").value = "";
}

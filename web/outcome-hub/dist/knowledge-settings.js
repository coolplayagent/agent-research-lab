import { KnowledgeBackend, KnowledgeAccess, OutcomeServiceKind } from "/assets/shared/contracts.js";
import { api } from "./api.js";
import { t, label } from "./i18n.js";
import { el, button, get, actions } from "./dom.js";
import { input, check, select, area, field, dialog, finish } from "./forms.js";
export function knowledgeRequest(config, service, request) {
    return api("/api/v1/knowledge", { service_id: service, configuration_revision: config.revision, request }, "manage-knowledge");
}
export function renderKnowledgeSettings(view, saveBindings) {
    let panel = document.getElementById("knowledge-targets");
    if (!panel) {
        panel = el("section");
        panel.id = "knowledge-targets";
        get("service-list").parentElement.append(panel);
    }
    panel.replaceChildren(el("h2", t("knowledge.title")), el("p", t("knowledge.note")));
    panel.append(button(t("knowledge.binding"), () => bindScope(view.configuration, saveBindings)));
    for (const [scope, bindings] of [["group", view.configuration.knowledge_groups], ["goal", view.configuration.knowledge_goals]]) {
        for (const [id, binding] of Object.entries(bindings || {})) {
            panel.append(el("p", `${t(scope === "group" ? "knowledge.group" : "knowledge.goal")} · ${id} → ${binding.service_id}/${binding.repository}`), actions(button(t("services.edit"), () => bindScope(view.configuration, saveBindings, scope, id)), button(t("services.unbind"), async () => { const next = structuredClone(view.configuration); delete (scope === "goal" ? next.knowledge_goals : next.knowledge_groups)?.[id]; await saveBindings(next); })));
        }
    }
    for (const service of view.configuration.outcome_services || []) {
        if (service.kind !== OutcomeServiceKind.Knowledge || !service.enabled)
            continue;
        const card = el("article", undefined, "card"), status = el("p"), targets = el("section");
        const load = async () => {
            const config = await knowledgeRequest(view.configuration, service.id, { operation: "list" });
            const save = async (next) => { await knowledgeRequest(view.configuration, service.id, { operation: "save", configuration: next }); await load(); };
            targets.replaceChildren(button(t("knowledge.add"), () => edit(config, save)));
            for (const [name, target] of Object.entries(config.targets)) {
                const row = el("article", undefined, "card");
                row.append(el("h4", name), el("p", `${label("KnowledgeBackend", target.kind)} · ${label("KnowledgeAccess", target.access)} · ${t(target.enabled ? "knowledge.enabled" : "knowledge.disabled")}`), el("code", `${target.path}/${target.content_root}`), actions(button(t("services.edit"), () => edit(config, save, name)), button(t("knowledge.probe"), async () => { const result = await knowledgeRequest(view.configuration, service.id, { operation: "probe", target: name }); status.textContent = t("knowledge.connected", { revision: result.revision }); }), button(t(target.enabled ? "knowledge.disable" : "knowledge.enable"), async () => { const next = structuredClone(config); next.targets[name].enabled = !target.enabled; await save(next); }), button(t("services.remove"), async () => { if (!confirm(t("ux.confirmRemove", { title: name })))
                    return; const next = structuredClone(config); delete next.targets[name]; await save(next); })));
                targets.append(row);
            }
        };
        card.append(el("h3", service.id), button(t("knowledge.load"), load), status, targets);
        panel.append(card);
    }
}
function edit(config, save, name) {
    const old = name ? config.targets[name] : undefined, box = dialog(t("knowledge.add"));
    const id = input(name || ""), path = input(old?.path || ""), root = input(old?.content_root || "knowledge"), kind = select(Object.values(KnowledgeBackend).map(v => [v, label("KnowledgeBackend", v)]), old?.kind || KnowledgeBackend.Git), access = select(Object.values(KnowledgeAccess).map(v => [v, label("KnowledgeAccess", v)]), old?.access || KnowledgeAccess.ReadOnly), enabled = check(old?.enabled ?? true), branch = input(old?.reference || "refs/heads/outcomes"), remote = input(old?.remote || ""), env = area(old?.credential_env.join("\n") || "");
    id.required = path.required = root.required = true;
    id.readOnly = !!name;
    box.form.append(field(t("knowledge.id"), id), field(t("knowledge.kind"), kind), field(t("knowledge.path"), path), field(t("knowledge.root"), root), field(t("knowledge.access"), access), field(t("outcome.enabled"), enabled));
    const git = el("section");
    git.append(field(t("knowledge.branch"), branch), field(t("knowledge.remote"), remote), field(t("knowledge.credentials"), env));
    box.form.append(git);
    const update = () => { git.hidden = kind.value !== KnowledgeBackend.Git; };
    kind.onchange = update;
    update();
    finish(box, async () => {
        const next = structuredClone(config), isGit = kind.value === KnowledgeBackend.Git;
        next.targets[id.value.trim()] = { kind: kind.value, path: path.value.trim(), content_root: root.value.trim(), access: access.value, enabled: enabled.checked, reference: isGit ? branch.value.trim() : null, remote: isGit && remote.value.trim() ? remote.value.trim() : null, credential_env: isGit ? env.value.split("\n").map(v => v.trim()).filter(Boolean) : [] };
        await save(next);
    });
}
function bindScope(config, save, oldScope, oldId) {
    const box = dialog(t("knowledge.binding")), scope = select([["group", t("knowledge.group")], ["goal", t("knowledge.goal")]], oldScope || "group"), id = input(oldId || ""), service = select((config.outcome_services || []).filter(s => s.kind === OutcomeServiceKind.Knowledge).map(s => [s.id, s.id]), ""), target = input("");
    scope.disabled = id.readOnly = !!oldId;
    id.required = service.required = target.required = true;
    const old = (oldScope === "goal" ? config.knowledge_goals : config.knowledge_groups)?.[oldId || ""];
    service.value = old?.service_id || service.options[0]?.value || "";
    target.value = old?.repository || "";
    box.form.append(field(t("knowledge.scope"), scope), field(t("knowledge.scopeId"), id), field(t("outcome.knowledge"), service), field(t("knowledge.id"), target), el("p", t("knowledge.bindingNote")));
    finish(box, async () => { const next = structuredClone(config), key = scope.value === "goal" ? "knowledge_goals" : "knowledge_groups"; next[key] ||= {}; next[key][id.value.trim()] = { service_id: service.value, repository: target.value.trim() }; await save(next); });
}

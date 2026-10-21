import { KnowledgeBackend, OutcomeServiceKind } from "/assets/shared/contracts.js";
import { knowledgeForm } from "./knowledge-form.js";
import { renderOutcomeServices } from "./outcome-service-settings.js";
import { api } from "./api.js";
import { t, label } from "./i18n.js";
import { el, button, get, actions } from "./dom.js";
import { input, select, field, dialog, finish } from "./forms.js";
export function knowledgeRequest(config, service, request) {
    return api("/api/v1/knowledge", { service_id: service, configuration_revision: config.revision, request }, "manage-knowledge", 75000);
}
export function renderKnowledgeSettings(view, saveBindings, refresh = async () => { }) {
    renderOutcomeServices(view, get("knowledge-services"), OutcomeServiceKind.Knowledge, saveBindings);
    const panel = get("knowledge-targets"), binding = button(t("knowledge.binding"), () => bindScope(view.configuration, saveBindings));
    const local = async (request) => {
        const latest = await api("/api/v1/collaboration/services");
        try {
            const response = await api("/api/v1/knowledge/local", { configuration_revision: latest.configuration.revision, request }, "manage-knowledge", 75000);
            return response.result;
        }
        finally {
            await refresh();
        }
    };
    const add = (kind) => knowledgeForm(kind, async (name, target) => { await local({ operation: "add", name, target }); }, target => local({ operation: "inspect", target }));
    const loadedTargets = new Map();
    binding.disabled = true;
    binding.title = t("knowledge.bindingPrerequisite");
    panel.replaceChildren(el("h2", t("knowledge.title")), el("p", t("knowledge.note")), actions(button(t("knowledge.addGit"), () => add(KnowledgeBackend.Git)), button(t("knowledge.addDirectory"), () => add(KnowledgeBackend.Directory)), binding));
    const available = (view.configuration.outcome_services || []).filter(s => s.kind === OutcomeServiceKind.Knowledge && s.enabled);
    if (!available.length)
        panel.append(el("p", t("knowledge.empty")));
    for (const [scope, bindings] of [["group", view.configuration.knowledge_groups], ["goal", view.configuration.knowledge_goals]]) {
        for (const [id, binding] of Object.entries(bindings || {})) {
            panel.append(el("p", `${t(scope === "group" ? "knowledge.group" : "knowledge.goal")} · ${id} → ${binding.service_id}/${binding.repository}`), actions(button(t("services.edit"), () => bindScope(view.configuration, saveBindings, scope, id)), button(t("services.unbind"), async () => { const next = structuredClone(view.configuration); delete (scope === "goal" ? next.knowledge_goals : next.knowledge_groups)?.[id]; await saveBindings(next); })));
        }
    }
    for (const [id, binding] of Object.entries(view.configuration.outcome_bindings || {})) {
        if (!binding.knowledge)
            continue;
        panel.append(el("p", `${t("research.topics")} · ${id} → ${binding.knowledge.service_id}/${binding.knowledge.repository}`), actions(button(t("services.edit"), () => bindScope(view.configuration, saveBindings, "topic", id)), button(t("services.unbind"), async () => {
            const next = structuredClone(view.configuration), entry = next.outcome_bindings[id];
            entry.knowledge = null;
            if (!Object.keys(entry.verifiers).length)
                delete next.outcome_bindings[id];
            await saveBindings(next);
        })));
    }
    for (const service of view.configuration.outcome_services || []) {
        if (service.kind !== OutcomeServiceKind.Knowledge || !service.enabled)
            continue;
        const card = el("article", undefined, "card"), status = el("p"), targets = el("section");
        let generation = 0;
        const load = async () => {
            const request = ++generation;
            const config = await knowledgeRequest(view.configuration, service.id, { operation: "list" });
            if (request !== generation || !panel.isConnected)
                return;
            const inspect = (target) => knowledgeRequest(view.configuration, service.id, { operation: "inspect", target });
            const save = async (next) => { await knowledgeRequest(view.configuration, service.id, { operation: "save", configuration: next }); await load(); };
            loadedTargets.set(service.id, Object.values(config.targets).some(target => target.enabled));
            binding.disabled = ![...loadedTargets.values()].some(Boolean);
            targets.replaceChildren(actions(button(t("knowledge.addGit"), () => edit(config, save, inspect, undefined, KnowledgeBackend.Git)), button(t("knowledge.addDirectory"), () => edit(config, save, inspect, undefined, KnowledgeBackend.Directory))));
            if (!Object.keys(config.targets).length)
                targets.append(el("p", t("knowledge.empty")));
            for (const [name, target] of Object.entries(config.targets)) {
                const row = el("article", undefined, "card");
                row.append(el("h4", name), el("p", `${label("KnowledgeBackend", target.kind)} · ${label("KnowledgeAccess", target.access)} · ${t(target.enabled ? "knowledge.enabled" : "knowledge.disabled")}`), el("code", target.content_root === "." ? target.path : `${target.path}/${target.content_root}`), actions(button(t("services.edit"), () => edit(config, save, inspect, name)), button(t("knowledge.probe"), async () => { const result = await knowledgeRequest(view.configuration, service.id, { operation: "probe", target: name }); status.textContent = t("knowledge.connected", { revision: result.revision }); }), button(t(target.enabled ? "knowledge.disable" : "knowledge.enable"), async () => { const next = structuredClone(config); next.targets[name].enabled = !target.enabled; await save(next); }), button(t("services.remove"), async () => { if (!confirm(t("ux.confirmRemove", { title: name })))
                    return; const next = structuredClone(config); delete next.targets[name]; await save(next); })));
                targets.append(row);
            }
        };
        card.append(el("h3", service.id), button(t("knowledge.load"), load), status, targets);
        panel.append(card);
        void load().catch(error => { status.textContent = t("knowledge.loadFailed", { error: String(error) }); });
    }
}
function edit(config, save, inspect, name, kind = KnowledgeBackend.Git) {
    const old = name ? config.targets[name] : undefined;
    knowledgeForm(old?.kind || kind, async (id, target) => {
        if (!name && config.targets[id])
            throw new Error(t("knowledge.duplicateId"));
        const prepared = await inspect(target);
        const next = structuredClone(config);
        next.targets[id] = prepared.target;
        await save(next);
    }, inspect, name, old);
}
function bindScope(config, save, oldScope, oldId) {
    const box = dialog(t("knowledge.binding"));
    const scope = select([["group", t("knowledge.group")], ["goal", t("knowledge.goal")], ["topic", t("research.topics")]], oldScope || "group");
    const id = input(oldId || ""), service = select((config.outcome_services || []).filter(s => s.kind === OutcomeServiceKind.Knowledge).map(s => [s.id, s.id]), ""), target = input("");
    scope.disabled = id.readOnly = !!oldId;
    id.required = service.required = target.required = true;
    const update = () => {
        const old = scope.value === "topic" ? config.outcome_bindings?.[id.value.trim()]?.knowledge
            : (scope.value === "goal" ? config.knowledge_goals : config.knowledge_groups)?.[id.value.trim()];
        service.value = old?.service_id || service.options[0]?.value || "";
        target.value = old?.repository || "";
    };
    scope.onchange = id.onchange = update;
    update();
    box.form.append(field(t("knowledge.scope"), scope), field(t("knowledge.scopeId"), id), field(t("outcome.knowledge"), service), field(t("knowledge.id"), target), el("p", t("knowledge.bindingNote")));
    finish(box, async () => {
        const next = structuredClone(config), binding = { service_id: service.value, repository: target.value.trim() };
        if (scope.value === "topic") {
            next.outcome_bindings ||= {};
            const entry = next.outcome_bindings[id.value.trim()] ||= { verifiers: {}, knowledge: null };
            entry.knowledge = binding;
        }
        else {
            const key = scope.value === "goal" ? "knowledge_goals" : "knowledge_groups";
            next[key] ||= {};
            next[key][id.value.trim()] = binding;
        }
        await save(next);
    });
}

import { OutcomeKind, OutcomeServiceKind } from "/assets/shared/contracts.js";
import { t, label } from "./i18n.js";
import { api, query } from "./api.js";
import { el, button, get, actions } from "./dom.js";
import { input, check, select, area, field, dialog, finish } from "./forms.js";
export function renderOutcomeSettings(view, save) {
    let panel = document.getElementById("outcome-services");
    if (!panel) {
        panel = el("section");
        panel.id = "outcome-services";
        get("service-list").parentElement.append(panel);
    }
    panel.replaceChildren(el("h2", t("outcome.services")), el("p", t("outcome.servicesNote")), button(t("outcome.addService"), () => editService(view.configuration, undefined, save)));
    const config = view.configuration;
    for (const service of config.outcome_services || []) {
        const card = el("article", undefined, "card"), health = view.outcome_health?.find(h => h.id === service.id);
        card.append(el("h3", service.id), el("p", `${label("OutcomeServiceKind", service.kind)} · ${health ? label("ServiceHealth", health.status) : t("services.unknown")}`), el("code", service.endpoint), el("p", service.outcome_kinds.map(k => label("OutcomeKind", k)).join(" · ")), actions(button(t("services.edit"), () => editService(config, service, save)), button(t("services.remove"), async () => {
            if (!confirm(t("ux.confirmRemove", { title: service.id })))
                return;
            const next = structuredClone(config);
            next.outcome_services = next.outcome_services?.filter(s => s.id !== service.id);
            await save(next);
        })));
        panel.append(card);
    }
    panel.append(el("h3", t("outcome.bindings")), button(t("outcome.bind"), () => editBinding(config, save)));
    for (const [topic, binding] of Object.entries(config.outcome_bindings || {})) {
        const card = el("article", undefined, "card");
        card.append(el("h4", topic), el("p", Object.entries(binding.verifiers).map(([kind, id]) => `${label("OutcomeKind", kind)} → ${id}`).join(" · ")), el("p", binding.knowledge ? `${binding.knowledge.service_id} → ${binding.knowledge.repository}` : t("outcome.unbound")), actions(button(t("services.edit"), () => editBinding(config, save, topic)), button(t("services.unbind"), async () => { const next = structuredClone(config); delete next.outcome_bindings?.[topic]; await save(next); })));
        panel.append(card);
    }
}
function editService(config, old, save) {
    const box = dialog(t("outcome.addService")), id = input(old?.id || ""), endpoint = input(old?.endpoint || ""), enabled = check(old?.enabled ?? true), kind = select(Object.values(OutcomeServiceKind).map(k => [k, label("OutcomeServiceKind", k)]), old?.kind || OutcomeServiceKind.Verifier), seconds = input(String(old?.max_seconds || 120), "number"), dependencies = area(old?.dependencies.join("\n") || "");
    id.required = endpoint.required = true;
    id.readOnly = !!old;
    seconds.min = "1";
    seconds.max = "600";
    box.form.append(field(t("research.id"), id), field(t("outcome.serviceKind"), kind), field(t("outcome.endpoint"), endpoint), field(t("outcome.enabled"), enabled), field(t("outcome.seconds"), seconds), field(t("outcome.dependencies"), dependencies));
    const kinds = Object.values(OutcomeKind).map(kind => ({ kind, node: check(old?.outcome_kinds.includes(kind) ?? true) }));
    for (const { kind, node } of kinds)
        box.form.append(field(label("OutcomeKind", kind), node));
    finish(box, async () => { const next = structuredClone(config); const entry = { id: id.value.trim(), endpoint: endpoint.value.trim(), kind: kind.value, enabled: enabled.checked, protocol_version: 1, outcome_kinds: kinds.filter(k => k.node.checked).map(k => k.kind), dependencies: dependencies.value.split("\n").map(v => v.trim()).filter(Boolean), max_seconds: Number(seconds.value) }; next.outcome_services = [...(next.outcome_services || []).filter(s => s.id !== entry.id), entry]; await save(next); });
}
async function editBinding(config, save, topicId) {
    const topics = [];
    let after = "";
    do {
        const page = await api("/api/im/topics?" + query({ after }));
        topics.push(...page.topics);
        after = page.next_after || "";
    } while (after && topics.length < 2048);
    const box = dialog(t("outcome.bind")), topic = select(topics.map(row => [row.topic.id, row.topic.title]), topicId || topics[0]?.topic.id || "");
    topic.required = true;
    topic.disabled = !!topicId;
    const old = topicId ? config.outcome_bindings?.[topicId] : undefined, services = config.outcome_services || [];
    box.form.append(field(t("research.topics"), topic), el("p", t("outcome.bindingsNote")));
    const verifiers = Object.values(OutcomeKind).map(kind => ({ kind, node: select([["", t("outcome.manualVerify")], ...services.filter(s => s.kind === OutcomeServiceKind.Verifier && s.outcome_kinds.includes(kind)).map(s => [s.id, s.id])], old?.verifiers[kind] || "") }));
    for (const { kind, node } of verifiers)
        box.form.append(field(label("OutcomeKind", kind), node));
    const knowledge = select([["", t("outcome.unbound")], ...services.filter(s => s.kind === OutcomeServiceKind.Knowledge).map(s => [s.id, s.id])], old?.knowledge?.service_id || ""), repository = input(old?.knowledge?.repository || "");
    box.form.append(field(t("outcome.knowledge"), knowledge), field(t("outcome.repositoryAlias"), repository), el("p", t("outcome.knowledgeNote")));
    finish(box, async () => { const next = structuredClone(config), binding = { verifiers: {}, knowledge: knowledge.value ? { service_id: knowledge.value, repository: repository.value.trim() } : null }; for (const { kind, node } of verifiers)
        if (node.value)
            binding.verifiers[kind] = node.value; next.outcome_bindings ||= {}; next.outcome_bindings[topic.value] = binding; await save(next); });
}

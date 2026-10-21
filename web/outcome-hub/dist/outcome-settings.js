import { OutcomeKind, OutcomeServiceKind } from "/assets/shared/contracts.js";
import { t, label } from "./i18n.js";
import { api, query } from "./api.js";
import { el, button, get, actions } from "./dom.js";
import { select, field, dialog, finish } from "./forms.js";
import { renderOutcomeServices } from "./outcome-service-settings.js";
export function renderOutcomeSettings(view, save) {
    let panel = document.getElementById("outcome-services");
    if (!panel) {
        panel = el("section");
        panel.id = "outcome-services";
        get("service-list").parentElement.append(panel);
    }
    const config = view.configuration;
    renderOutcomeServices(view, panel, OutcomeServiceKind.Verifier, save);
    panel.append(el("h3", t("outcome.bindings")), button(t("outcome.bind"), () => editBinding(config, save)));
    for (const [topic, binding] of Object.entries(config.outcome_bindings || {})) {
        if (!Object.keys(binding.verifiers).length)
            continue;
        const card = el("article", undefined, "card");
        card.append(el("h4", topic), el("p", Object.entries(binding.verifiers).map(([kind, id]) => `${label("OutcomeKind", kind)} → ${id}`).join(" · ")), actions(button(t("services.edit"), () => editBinding(config, save, topic)), button(t("services.unbind"), async () => { const next = structuredClone(config); if (next.outcome_bindings?.[topic].knowledge)
            next.outcome_bindings[topic].verifiers = {};
        else
            delete next.outcome_bindings?.[topic]; await save(next); })));
        panel.append(card);
    }
}
async function editBinding(config, save, topicId) {
    const topics = [];
    let after = "";
    do {
        const page = await api("/api/v1/collaboration/topics?" + query({ after }));
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
    topic.onchange = () => {
        for (const { kind, node } of verifiers)
            node.value = config.outcome_bindings?.[topic.value]?.verifiers[kind] || "";
    };
    topic.onchange(new Event("change"));
    finish(box, async () => { const next = structuredClone(config), binding = { verifiers: {}, knowledge: config.outcome_bindings?.[topic.value]?.knowledge || null }; for (const { kind, node } of verifiers)
        if (node.value)
            binding.verifiers[kind] = node.value; next.outcome_bindings ||= {}; next.outcome_bindings[topic.value] = binding; await save(next); });
}

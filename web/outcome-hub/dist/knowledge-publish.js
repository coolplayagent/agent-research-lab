import { uuid } from "/assets/shared/identity.js";
import { OutcomeServiceKind, KnowledgeAccess } from "/assets/shared/contracts.js";
import { api } from "./api.js";
import { t } from "./i18n.js";
import { el, button } from "./dom.js";
import { select, field, dialog, finish } from "./forms.js";
import { knowledgeRequest } from "./knowledge-settings.js";
export async function publishKnowledge(config, view, reload) {
    const goal = await api("/api/v1/collaboration/goals?id=" + encodeURIComponent(view.outcome.input.goal_id));
    const preferred = config.knowledge_goals?.[goal.input.id] || config.knowledge_groups?.[goal.input.group_id];
    const box = dialog(t("outcome.write")), services = (config.outcome_services || []).filter(s => s.enabled && s.kind === OutcomeServiceKind.Knowledge), service = select(services.map(s => [s.id, s.id]), preferred?.service_id || services[0]?.id || ""), target = select([], ""), content = el("section");
    service.required = target.required = true;
    let preview;
    const reset = () => { preview = undefined; content.replaceChildren(); };
    const load = async () => { reset(); const requested = service.value; const current = await knowledgeRequest(config, requested, { operation: "list" }); if (requested !== service.value)
        return; const next = select(Object.entries(current.targets).filter(([, t]) => t.enabled && t.access === KnowledgeAccess.ReadWrite).map(([id]) => [id, id]), ""); target.replaceChildren(...Array.from(next.children)); target.value = preferred?.service_id === service.value && current.targets[preferred.repository]?.enabled ? preferred.repository : target.options[0]?.value || ""; };
    service.onchange = () => { load().catch(error => { box.status.textContent = String(error); }); };
    target.onchange = reset;
    box.form.append(field(t("outcome.knowledge"), service), field(t("knowledge.id"), target), button(t("knowledge.preview"), async () => {
        const selectedService = service.value, selectedTarget = target.value;
        const value = await knowledgeRequest(config, selectedService, { operation: "preview", target: selectedTarget, outcome: view.outcome });
        if (service.value !== selectedService || target.value !== selectedTarget)
            return;
        preview = value;
        content.replaceChildren(el("p", preview.path), el("h4", t("knowledge.before")), el("pre", preview.before || t("knowledge.new")), el("h4", t("knowledge.after")), el("pre", preview.after));
    }), content, el("p", t("outcome.writeNote")));
    const request_id = "write-" + uuid();
    finish(box, async () => { if (!preview)
        throw new Error(t("knowledge.previewRequired")); await api("/api/v1/collaboration/outcomes/write", { request_id, outcome_id: view.outcome.input.id, expected_revision: preview.expected_revision, configuration_revision: config.revision, service_id: service.value, repository: target.value }, "manage-outcomes"); await reload(); });
    await load();
}

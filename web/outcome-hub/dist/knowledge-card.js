import { KnowledgeBackend } from "/assets/shared/contracts.js";
import { t, label } from "./i18n.js";
import { el, button, actions } from "./dom.js";
export function knowledgeCard(name, target, handlers) {
    const card = el("article", undefined, "card knowledge-card"), heading = el("div", undefined, "knowledge-card-heading"), identity = el("div"), badges = el("div", undefined, "knowledge-badges");
    identity.append(el("h4", name), el("small", label("KnowledgeBackend", target.kind)));
    heading.append(identity, el("span", t(target.enabled ? "knowledge.enabled" : "knowledge.disabled"), target.enabled ? "knowledge-badge is-enabled" : "knowledge-badge"));
    badges.append(el("span", label("KnowledgeAccess", target.access), "knowledge-badge knowledge-access"));
    const details = el("dl", undefined, "knowledge-details");
    const detail = (key, value) => { const dd = el("dd"); dd.append(el("code", value)); details.append(el("dt", t(key)), dd); };
    detail("knowledge.location", target.path);
    detail("knowledge.root", target.content_root);
    if (target.kind === KnowledgeBackend.Git && target.reference)
        detail("knowledge.branch", target.reference);
    const status = el("p", undefined, "knowledge-card-status");
    status.setAttribute("role", "status");
    const edit = button(t("services.edit"), handlers.edit);
    edit.className = "knowledge-edit";
    const probe = button(t("knowledge.probe"), async () => {
        status.textContent = t("knowledge.checkingPath");
        try {
            const result = await handlers.probe();
            status.textContent = t("knowledge.connected", { revision: result.revision });
        }
        catch (error) {
            status.textContent = t("knowledge.pathFailed", { error: String(error) });
        }
    });
    const remove = button(t("services.remove"), async () => {
        try {
            await handlers.remove();
        }
        catch (error) {
            status.textContent = String(error instanceof Error ? error.message : error);
        }
    });
    remove.className = "knowledge-danger";
    card.append(heading, badges, details);
    if (target.kind === KnowledgeBackend.Git && (target.remote || target.credential_env.length)) {
        const advanced = el("details", undefined, "knowledge-extra"), values = el("dl", undefined, "knowledge-details");
        advanced.append(el("summary", t("knowledge.connectionDetails")));
        if (target.remote)
            values.append(el("dt", t("knowledge.remote")), el("dd", target.remote));
        if (target.credential_env.length)
            values.append(el("dt", t("knowledge.credentials")), el("dd", target.credential_env.join(", ")));
        advanced.append(values);
        card.append(advanced);
    }
    card.append(status, actions(edit, probe, button(t(target.enabled ? "knowledge.disable" : "knowledge.enable"), handlers.toggle), remove));
    return card;
}

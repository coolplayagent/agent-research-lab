import { uuid } from "/assets/shared/identity.js";
import { OutcomeAcceptanceState, OutcomeVerificationState, OutcomePublicationState, OutcomeReferenceKind, OutcomeServiceKind, KnowledgeWriteState } from "/assets/shared/contracts.js";
import { api, query } from "./api.js";
import { t, label } from "./i18n.js";
import { el, button, actions } from "./dom.js";
import { dialog, area, input, select, field, finish } from "./forms.js";
import { outcomeCommand, publicationForm } from "./outcome-forms.js";
export async function outcomePaths(topic_id) {
    const paths = [];
    let after = "";
    do {
        const page = await outcomeCommand({ operation: "paths", topic_id, after });
        paths.push(...page.paths);
        after = page.next_after || "";
    } while (after && paths.length < 4096);
    return paths;
}
export function outcomeLink(id, title = id) {
    const link = el("a", title);
    link.href = "#topics/outcomes/" + encodeURIComponent(id);
    link.onclick = () => { if (location.hash === link.hash)
        void showOutcome(id).catch(error => { document.getElementById("notice").textContent = String(error); }); };
    return link;
}
export async function showOutcome(id) {
    const view = await api("/api/im/outcomes?" + query({ id }));
    document.getElementById("outcome-dialog")?.remove();
    const box = dialog(view.outcome.input.title), entry = view.outcome.input;
    box.dialog.id = "outcome-dialog";
    box.dialog.classList.add("outcome-dialog");
    box.form.append(el("p", `${label("OutcomeKind", entry.kind)} · ${entry.goal_id} · ${view.outcome.author_id}`), el("p", [label("OutcomePublicationState", view.publication), label("OutcomeVerificationState", view.verification), label("OutcomeAcceptanceState", view.acceptance)].join(" · ")), el("p", entry.summary), el("h3", t("outcome.conditions")), el("p", entry.conditions), el("h3", t("outcome.evidence")));
    for (const evidence of entry.evidence)
        box.form.append(el("pre", `${evidence.repository}\n${evidence.commit}\n${evidence.path}\nsha256: ${evidence.sha256}`));
    const pins = el("details");
    pins.append(el("summary", t("outcome.pins")), el("pre", JSON.stringify({ pins: entry.pins, knowledge: entry.knowledge, digest: view.outcome.digest }, null, 2)));
    box.form.append(pins);
    if (entry.supersedes) {
        const p = el("p", t("outcome.supersedes") + " ");
        p.append(outcomeLink(entry.supersedes));
        box.form.append(p);
    }
    box.form.append(el("h3", t("outcome.references")));
    for (const reference of entry.references) {
        const p = el("p", label("OutcomeReferenceKind", reference.kind) + " · ");
        p.append(outcomeLink(reference.outcome_id), document.createTextNode(" · " + reference.reason));
        box.form.append(p);
    }
    if (!entry.references.length)
        box.form.append(el("p", t("outcome.noReferences")));
    if (view.withdrawal_reason)
        box.form.append(el("p", view.withdrawal_reason));
    if (view.latest_verification?.result)
        box.form.append(el("h3", t("outcome.verifierResult")), el("p", view.latest_verification.result.summary), el("pre", JSON.stringify(view.latest_verification.result.evidence, null, 2)));
    if (view.latest_decision)
        box.form.append(el("h3", t("outcome.decision")), el("p", view.latest_decision.reason));
    const reload = async () => { box.dialog.close(); await showOutcome(id); };
    const controls = actions(button(t("outcome.refresh"), reload), button(t("outcome.copyLink"), async () => { await navigator.clipboard.writeText(location.href.split("#")[0] + "#topics/outcomes/" + encodeURIComponent(id)); box.status.textContent = t("outcome.linkCopied"); }));
    controls.append(button(t("outcome.reuse"), async () => { const paths = await outcomePaths(entry.topic_id); box.dialog.close(); publicationForm({ id: entry.topic_id }, paths, async () => showOutcome(id), { outcome_id: id, kind: OutcomeReferenceKind.Uses, reason: "" }); }));
    controls.append(button(t("outcome.newVersion"), async () => { const paths = await outcomePaths(entry.topic_id); box.dialog.close(); publicationForm({ id: entry.topic_id }, paths, async () => showOutcome(id), undefined, entry); }));
    if (view.publication === OutcomePublicationState.Published) {
        controls.append(button(t("outcome.withdraw"), () => reasonForm(t("outcome.withdraw"), async (reason) => { await outcomeCommand({ operation: "withdraw", id, reason }); await reload(); })));
        const accept = button(t("outcome.accept"), () => decision(OutcomeAcceptanceState.Accepted));
        accept.disabled = view.verification !== OutcomeVerificationState.Passed;
        if (accept.disabled)
            accept.title = t("outcome.acceptNote");
        controls.append(accept, button(t("outcome.return"), () => decision(OutcomeAcceptanceState.Returned)));
    }
    function decision(state) {
        reasonForm(t("outcome.decision"), async (reason) => { await outcomeCommand({ operation: "decide", decision: { id: "decision-" + uuid(), outcome_id: id, verification_id: view.latest_verification?.input.id || null, state, reason } }); await reload(); });
    }
    box.form.append(controls, el("p", t("outcome.acceptNote")), box.status);
    const servicePanel = el("section");
    box.form.append(servicePanel);
    const history = el("section");
    let after = 0;
    const more = button(t("outcome.history"), async () => {
        const page = await outcomeCommand({ operation: "history", id, after });
        for (const event of page.events) {
            const row = el("details");
            row.append(el("summary", `${new Date(event.created_ms).toLocaleString()} · ${label("OutcomeEventKind", event.kind)}`), el("pre", JSON.stringify(event.record, null, 2)));
            history.append(row);
        }
        after = page.next_after || 0;
        more.hidden = page.next_after === null;
    });
    box.form.append(history, more);
    box.dialog.showModal();
    try {
        const services = await api("/api/im/services");
        if (box.dialog.isConnected)
            renderServices(servicePanel, view, services, reload);
    }
    catch {
        if (box.dialog.isConnected)
            servicePanel.append(el("p", t("outcome.noServices")));
    }
}
function reasonForm(title, save) {
    const box = dialog(title), reason = area("");
    reason.required = true;
    reason.maxLength = 2048;
    box.form.append(field(t("outcome.reason"), reason));
    finish(box, () => save(reason.value.trim()));
}
function renderServices(panel, view, services, reload) {
    const config = services.configuration, entry = view.outcome.input;
    const verifiers = (config.outcome_services || []).filter(s => s.kind === OutcomeServiceKind.Verifier && s.outcome_kinds.includes(entry.kind));
    panel.append(el("h3", t("outcome.verify")));
    if (verifiers.length && view.publication === OutcomePublicationState.Published) {
        const selected = select(verifiers.map(s => [s.id, s.id]), config.outcome_bindings?.[entry.topic_id]?.verifiers[entry.kind] || verifiers[0].id);
        panel.append(field(t("outcome.verifier"), selected), button(t("outcome.verify"), async () => { await api("/api/im/outcomes/verify", { request_id: "verify-" + uuid(), outcome_id: entry.id, verifier_id: selected.value, configuration_revision: config.revision }, "manage-outcomes"); await reload(); }));
    }
    else
        panel.append(el("p", t("outcome.noVerifier")));
    const target = config.outcome_bindings?.[entry.topic_id]?.knowledge;
    panel.append(el("h3", t("outcome.knowledge")), el("p", t("outcome.knowledgeNote")));
    const write = view.latest_write;
    if (write) {
        const state = write.result?.state || (write.deadline_ms > Date.now() ? KnowledgeWriteState.Pending : KnowledgeWriteState.Unknown);
        panel.append(el("p", label("KnowledgeWriteState", state)), el("p", write.result?.summary || ""));
        if (write.result?.reference)
            panel.append(el("pre", write.result.reference + "\n" + write.result.revision));
        if ((state === KnowledgeWriteState.Pending || state === KnowledgeWriteState.Unknown))
            panel.append(button(t("outcome.reconcile"), async () => { await api("/api/im/outcomes/reconcile", { outcome_id: entry.id, request_id: write.input.id }, "manage-outcomes"); await reload(); }));
    }
    if (!target) {
        panel.append(el("p", t("outcome.unbound")));
        return;
    }
    panel.append(el("p", target.repository));
    const publish = button(t("outcome.write"), () => {
        const box = dialog(t("outcome.write")), revision = input("");
        revision.required = true;
        box.form.append(el("p", target.repository), field(t("outcome.expectedRevision"), revision), el("p", t("outcome.writeNote")));
        const request_id = "write-" + uuid();
        finish(box, async () => { await api("/api/im/outcomes/write", { request_id, outcome_id: entry.id, expected_revision: revision.value.trim(), configuration_revision: config.revision }, "manage-outcomes"); await reload(); });
    });
    publish.disabled = view.acceptance !== OutcomeAcceptanceState.Accepted || !!(write && (!write.result || (write.result.state === KnowledgeWriteState.Pending || write.result.state === KnowledgeWriteState.Unknown)));
    panel.append(publish);
}

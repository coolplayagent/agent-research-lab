import { uuid } from "/assets/shared/identity.js";
import { OutcomeKind, OutcomeReferenceKind } from "/assets/shared/contracts.js";
import { api, query } from "./api.js";
import { t, label } from "./i18n.js";
import { element, select, input, area, dialog, finish, field } from "./forms.js";
import { button } from "./dom.js";
export const outcomeCommand = (value) => api("/api/im/outcomes", value, "manage-outcomes");
export async function pathForm(topic, done) {
    const goals = [];
    let after = "";
    do {
        const page = await api("/api/im/goals?" + query({ group_id: topic.group_id, after }));
        goals.push(...page.goals);
        after = page.next_after || "";
    } while (after && goals.length < 4096);
    const box = dialog(t("outcome.pathForm")), goal = select(goals.map(g => [g.input.id, g.input.title]), goals[0]?.input.id || ""), name = input(""), hypothesis = area("");
    for (const node of [goal, name, hypothesis])
        node.required = true;
    box.form.append(element("p", t("outcome.pathNote")), field(t("research.goal"), goal), field(t("outcome.pathName"), name), field(t("outcome.hypothesis"), hypothesis));
    if (!goals.length)
        box.form.append(element("p", t("outcome.noGoals")));
    finish(box, async () => {
        await outcomeCommand({ operation: "link_path", path: { topic_id: topic.id, goal_id: goal.value, name: name.value.trim(), hypothesis: hypothesis.value.trim() } });
        await done();
    });
}
export function publicationForm(topic, paths, done, reference, old) {
    const box = dialog(t("outcome.publish")), goal = select(paths.map(p => [p.goal_id, p.name]), old?.goal_id || paths[0]?.goal_id || ""), kind = select(Object.values(OutcomeKind).map(k => [k, label("OutcomeKind", k)]), old?.kind || OutcomeKind.Finding), title = input(old?.title || ""), summary = area(old?.summary || ""), conditions = area(old?.conditions || "");
    for (const node of [goal, title, summary, conditions])
        node.required = true;
    title.maxLength = 256;
    summary.maxLength = 4096;
    conditions.maxLength = 4096;
    box.form.append(element("p", t("outcome.publishNote")), field(t("outcome.path"), goal), field(t("outcome.kind"), kind), field(t("outcome.title"), title), field(t("outcome.summary"), summary), field(t("outcome.conditions"), conditions));
    const evidence = element("section"), rows = [];
    const addEvidence = () => {
        if (rows.length >= 16)
            return;
        const row = { repository: input(""), commit: input(""), path: input(""), sha256: input("") }, card = element("fieldset");
        card.append(element("legend", t("outcome.evidence")));
        for (const [key, node] of Object.entries(row)) {
            node.required = true;
            card.append(field(t("outcome.evidence." + key), node));
        }
        row.commit.pattern = "([a-f0-9]{40}|[a-f0-9]{64})";
        row.sha256.pattern = "[a-f0-9]{64}";
        rows.push(row);
        evidence.append(card);
    };
    addEvidence();
    box.form.append(element("p", t("outcome.evidenceNote")), evidence, button(t("outcome.addEvidence"), addEvidence));
    const pins = { source: input(old?.pins.source || ""), prompt: input(old?.pins.prompt || ""), policy: input(old?.pins.policy || "") };
    for (const [key, node] of Object.entries(pins)) {
        node.required = true;
        node.pattern = key === "source" ? "([a-f0-9]{40}|[a-f0-9]{64})" : "[a-f0-9]{64}";
        box.form.append(field(t("research.pin." + key), node));
    }
    const parent = input(reference?.outcome_id || ""), relation = select(Object.values(OutcomeReferenceKind).map(k => [k, label("OutcomeReferenceKind", k)]), reference?.kind || OutcomeReferenceKind.Uses), reason = area(reference?.reason || "");
    box.form.append(field(t("outcome.referenceId"), parent), field(t("outcome.relation"), relation), field(t("outcome.referenceReason"), reason));
    const id = "outcome-" + uuid();
    finish(box, async () => {
        if (parent.value && !reason.value.trim())
            throw new Error(t("outcome.reasonRequired"));
        const value = { id, topic_id: topic.id, goal_id: goal.value, kind: kind.value, title: title.value.trim(), summary: summary.value.trim(), conditions: conditions.value.trim(),
            evidence: rows.map(row => ({ repository: row.repository.value.trim(), commit: row.commit.value.trim(), path: row.path.value.trim(), sha256: row.sha256.value.trim() })),
            references: parent.value.trim() ? [{ outcome_id: parent.value.trim(), kind: relation.value, reason: reason.value.trim() }] : [],
            supersedes: old?.id || null, attempt: null, pins: { source: pins.source.value.trim(), prompt: pins.prompt.value.trim(), policy: pins.policy.value.trim(), superpod: null }, knowledge: null };
        await outcomeCommand({ operation: "publish", input: value });
        await done();
    });
}

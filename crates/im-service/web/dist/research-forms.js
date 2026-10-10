import { TopicState, SubjectKind, GroupKind, } from "/assets/shared/contracts.js";
import { api, query } from "./api.js";
import { t, label } from "./i18n.js";
import { element, select, input, area, dialog, finish, field, } from "./forms.js";
const save = (value) => api("/api/im/research", value, "manage-research");
export async function topicForm(old, done) {
    const box = dialog(t("research.topicForm")), id = input(old?.id || "topic-" + crypto.randomUUID()), title = input(old?.title || ""), objective = area(old?.objective || ""), state = select(Object.values(TopicState).map((s) => [s, label("TopicState", s)]), old?.state || TopicState.Active);
    const groups = (await api("/api/crystal/view")).directory.groups.filter((g) => !g.archived && g.kind !== GroupKind.Board);
    const group = input(old?.group_id || groups[0]?.id || ""), options = element("datalist");
    options.id = "topic-group-options";
    for (const row of groups) {
        const option = element("option", row.title);
        option.value = row.id;
        options.append(option);
    }
    group.setAttribute("list", options.id);
    group.readOnly = !!old;
    id.readOnly = !!old;
    for (const node of [id, title, objective, group])
        node.required = true;
    box.form.append(field(t("research.id"), id), field(t("research.title"), title), field(t("research.objective"), objective), field(t("research.group"), group), options, element("p", t("research.groupNote")), field(t("research.state"), state));
    finish(box, async () => {
        await save({
            operation: "topic",
            topic: {
                id: id.value.trim(),
                title: title.value.trim(),
                objective: objective.value.trim(),
                group_id: group.value.trim(),
                state: state.value,
                revision: old?.revision || 0,
            },
        });
        await done();
    });
}
export function subjectForm(topic, old, done) {
    const box = dialog(t("research.subjectForm")), id = input(old?.id || "subject-" + crypto.randomUUID()), name = input(old?.name || ""), kind = select(Object.values(SubjectKind).map((s) => [s, label("SubjectKind", s)]), old?.kind || SubjectKind.Repository), reference = input(old?.reference || "");
    id.readOnly = !!old;
    for (const node of [id, name, reference])
        node.required = true;
    box.form.append(element("p", topic.title), field(t("research.id"), id), field(t("research.name"), name), field(t("research.kind"), kind), field(t("research.reference"), reference), element("p", t("research.referenceNote")));
    finish(box, async () => {
        await save({
            operation: "subject",
            subject: {
                id: id.value.trim(),
                topic_id: topic.id,
                name: name.value.trim(),
                kind: kind.value,
                reference: reference.value.trim(),
                revision: old?.revision || 0,
            },
        });
        await done();
    });
}
export async function nodeForm(subject, group, nodes, done) {
    const goals = [];
    let after = "";
    do {
        const page = await api("/api/im/goals?" + query({ group_id: group, after }));
        goals.push(...page.goals);
        after = page.next_after || "";
    } while (after && goals.length < 4096);
    const box = dialog(t("research.nodeForm")), id = input("version-" + crypto.randomUUID()), version = input(""), reference = input(subject.reference), goal = select([
        ["", t("research.unassessed")],
        ...goals.map((g) => [g.input.id, g.input.title]),
    ], ""), parents = area("");
    for (const node of [id, version, reference])
        node.required = true;
    const available = nodes.filter((n) => n.subject.id === subject.id);
    parents.placeholder = available
        .slice(-4)
        .map((n) => n.node.input.id)
        .join("\n");
    box.form.append(element("p", subject.name), field(t("research.id"), id), field(t("research.version"), version), field(t("research.reference"), reference), field(t("research.parents"), parents), element("p", t("research.parentsNote")), field(t("research.goal"), goal), element("p", t("research.evidenceNote")));
    const pins = {
        source: input(""),
        prompt: input(""),
        policy: input(""),
        superpod: input(""),
    };
    for (const [key, node] of Object.entries(pins))
        box.form.append(field(t("research.pin." + key), node));
    finish(box, async () => {
        const input = {
            id: id.value.trim(),
            subject_id: subject.id,
            parents: parents.value
                .split("\n")
                .map((s) => s.trim())
                .filter(Boolean),
            version: version.value.trim(),
            reference: reference.value.trim(),
            goal_id: goal.value || null,
            pins: {
                source: pins.source.value.trim() || null,
                prompt: pins.prompt.value.trim() || null,
                policy: pins.policy.value.trim() || null,
                superpod: pins.superpod.value.trim() || null,
            },
        };
        await save({ operation: "node", node: input });
        await done();
    });
}

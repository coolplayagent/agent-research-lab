import { uuid } from "/assets/shared/identity.js";
import { TopicState, SubjectKind, GroupKind, } from "/assets/shared/contracts.js";
import { api, query } from "./api.js";
import { t, label } from "./i18n.js";
import { element, select, input, area, dialog, finish, field, } from "./forms.js";
const save = (value) => api("/api/v1/collaboration/coordination", value, "manage-research");
export async function topicForm(old, done) {
    const groups = [];
    let cursor = "", count = 0;
    do {
        const page = await api("/api/v1/messaging/view?" + query({ after: cursor }));
        groups.push(...page.directory.groups.filter(g => !g.archived && g.kind !== GroupKind.Board));
        count += page.directory.groups.length;
        const next = page.directory.next_after || "";
        if (next && (next === cursor || count >= 8192))
            throw new Error(t("workspace.topicLimit"));
        cursor = next;
    } while (cursor);
    const box = dialog(t("research.topicForm")), id = input(old?.id || "topic-" + uuid()), title = input(old?.title || ""), objective = area(old?.objective || ""), state = select(Object.values(TopicState).map((s) => [s, label("TopicState", s)]), old?.state || TopicState.Active);
    const options = groups.map(group => [group.id, group.title]);
    if (old && !groups.some(group => group.id === old.group_id))
        options.push([old.group_id, old.group_id]);
    const group = select(options, old?.group_id || groups[0]?.id || "");
    group.disabled = !!old;
    id.readOnly = !!old;
    for (const node of [id, title, objective, group])
        node.required = true;
    box.form.append(field(t("research.id"), id), field(t("research.title"), title), field(t("research.objective"), objective), field(t("research.group"), group), element("p", t("research.groupNote")), field(t("research.state"), state));
    if (!groups.length && !old) {
        const note = element("p", t("layout.topicPrerequisite"));
        const link = element("a", t("workspace.conversations"));
        link.href = "#groups";
        link.onclick = () => box.dialog.close();
        note.append(link);
        box.form.append(note);
    }
    finish(box, async () => {
        const updated = await api("/api/v1/collaboration/coordination", {
            operation: "topic",
            topic: {
                id: id.value.trim(),
                title: title.value.trim(),
                objective: objective.value.trim(),
                group_id: group.value.trim(),
                state: state.value,
                revision: old?.revision || 0,
            },
        }, "manage-research");
        await done(updated);
    });
}
export function subjectForm(topic, old, done) {
    const box = dialog(t("research.subjectForm")), id = input(old?.id || "subject-" + uuid()), name = input(old?.name || ""), kind = select(Object.values(SubjectKind).map((s) => [s, label("SubjectKind", s)]), old?.kind || SubjectKind.Repository), reference = input(old?.reference || "");
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

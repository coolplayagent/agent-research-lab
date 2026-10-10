import { TopicState } from "/assets/shared/contracts.js";
import { api, query } from "./api.js";
import { get, el, button, notice } from "./dom.js";
import { t, label } from "./i18n.js";
import { openGroup as selectConversation } from "./conversation.js";
import { topicForm, subjectForm } from "./research-forms.js";
let after = null, selected, subjectAfter = null;
export async function loadTopics(more = false) {
    try {
        const page = await api("/api/im/topics?" +
            query({
                after: more ? after || "" : "",
                query: get("topic-search").value,
            }));
        if (!more)
            get("topic-list").replaceChildren();
        for (const row of page.topics) {
            const card = el("article", undefined, "card"), topic = row.topic;
            card.append(el("h3", topic.title), el("p", topic.objective), el("p", `${label("TopicState", topic.state)} · ${t("research.counts", { p0: row.subject_count, p1: row.goal_count })}`), button(t("research.open"), () => void openTopic(topic)), button(t("research.edit"), () => void topicForm(topic, async () => {
                await loadTopics();
                if (selected?.id === topic.id) {
                    const updated = (await api("/api/im/topics?" + query({ query: topic.title }))).topics.find((r) => r.topic.id === topic.id)?.topic;
                    if (updated)
                        await openTopic(updated);
                }
            })));
            get("topic-list").append(card);
        }
        if (!more && !page.topics.length)
            get("topic-list").append(el("p", t("research.noTopics")));
        after = page.next_after;
        get("more-topics").hidden = !after;
    }
    catch (error) {
        notice(error);
    }
}
async function openTopic(topic) {
    selected = topic;
    get("topic-detail").hidden = false;
    get("topic-detail-title").textContent = topic.title;
    get("topic-detail-objective").textContent = topic.objective;
    get("new-subject").disabled =
        topic.state === TopicState.Archived;
    await loadSubjects();
}
async function loadSubjects(more = false) {
    if (!selected)
        return;
    try {
        const page = await api("/api/im/subjects?" +
            query({ topic_id: selected.id, after: more ? subjectAfter || "" : "" }));
        if (!more)
            get("subject-list").replaceChildren();
        for (const row of page.subjects) {
            const subject = row.subject, card = el("article", undefined, "card");
            card.append(el("h4", subject.name), el("p", `${label("SubjectKind", subject.kind)} · ${t("research.versions", { p0: row.node_count })}`), el("code", subject.reference), button(t("research.graph"), () => {
                sessionStorage.setItem("evolution-subject", subject.id);
                location.hash = "evolution";
            }), button(t("research.edit"), () => subjectForm(selected, subject, () => loadSubjects())));
            get("subject-list").append(card);
        }
        if (!more && !page.subjects.length)
            get("subject-list").append(el("p", t("research.noSubjects")));
        subjectAfter = page.next_after;
        get("more-subjects").hidden = !subjectAfter;
    }
    catch (error) {
        notice(error);
    }
}
export function initTopics() {
    get("new-topic").onclick = () => void topicForm(undefined, () => loadTopics()).catch(notice);
    get("topic-search-button").onclick = () => void loadTopics();
    get("more-topics").onclick = () => void loadTopics(true);
    get("more-subjects").onclick = () => void loadSubjects(true);
    get("new-subject").onclick = () => {
        if (selected)
            subjectForm(selected, undefined, () => loadSubjects());
    };
    get("topic-open-group").onclick = () => {
        if (selected) {
            location.hash = "groups";
            void selectConversation(selected.group_id);
        }
    };
    get("topic-open-graph").onclick = () => {
        if (selected) {
            sessionStorage.setItem("evolution-topic", selected.id);
            location.hash = "evolution";
        }
    };
}

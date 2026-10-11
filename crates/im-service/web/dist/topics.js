import { revealResearch } from "./workspace.js";
import { loadEvolution } from "./evolution.js";
import { TopicState } from "/assets/shared/contracts.js";
import { api, query } from "./api.js";
import { get, el, button, notice, actions } from "./dom.js";
import { t, label } from "./i18n.js";
import { icon } from "./icons.js";
import { openGroup as selectConversation } from "./conversation.js";
import { topicForm, subjectForm } from "./research-forms.js";
let after = null, selected, subjectAfter = null;
let topicRequest = 0, subjectRequest = 0;
export async function loadTopics(more = false) {
    const request = ++topicRequest;
    try {
        const page = await api("/api/im/topics?" +
            query({
                after: more ? after || "" : "",
                query: get("topic-search").value,
            }));
        if (request !== topicRequest)
            return;
        if (!more)
            get("topic-list").replaceChildren();
        for (const row of page.topics) {
            const card = el("article", undefined, "card"), topic = row.topic;
            const heading = el("header", undefined, "identity-heading");
            const symbol = el("span", undefined, "topic-symbol");
            symbol.append(icon("topic"));
            heading.append(symbol, el("h3", topic.title));
            card.append(heading, el("p", topic.objective), el("p", `${label("TopicState", topic.state)} · ${t("research.counts", { p0: row.subject_count, p1: row.goal_count })}`), actions(button(t("research.open"), () => openTopic(topic)), button(t("research.edit"), () => topicForm(topic, async (updated) => {
                await loadTopics();
                if (selected?.id === updated.id)
                    await openTopic(updated);
            }))));
            get("topic-list").append(card);
        }
        if (!more && !page.topics.length)
            get("topic-list").append(el("p", t("research.noTopics")));
        after = page.next_after;
        get("more-topics").hidden = !after;
        const current = page.topics.find(row => row.topic.id === selected?.id)?.topic;
        if (current)
            await openTopic(current, false);
    }
    catch (error) {
        if (request === topicRequest)
            notice(error);
    }
}
async function openTopic(topic, reveal = true) {
    selected = topic;
    get("subject-list").replaceChildren(el("p", t("ux.loading")));
    get("topic-detail").hidden = false;
    get("topic-detail-title").textContent = topic.title;
    get("topic-detail-objective").textContent = topic.objective;
    get("new-subject").disabled =
        topic.state === TopicState.Archived;
    await loadSubjects();
    if (reveal && selected?.id === topic.id)
        get("topic-detail").scrollIntoView({ block: "nearest" });
}
async function loadSubjects(more = false) {
    if (!selected)
        return;
    const owner = selected, request = ++subjectRequest;
    try {
        const page = await api("/api/im/subjects?" +
            query({ topic_id: owner.id, after: more ? subjectAfter || "" : "" }));
        if (request !== subjectRequest || selected?.id !== owner.id)
            return;
        if (!more)
            get("subject-list").replaceChildren();
        for (const row of page.subjects) {
            const subject = row.subject, card = el("article", undefined, "card");
            const edit = button(t("research.edit"), () => subjectForm(owner, subject, async () => { if (selected?.id === owner.id)
                await loadSubjects(); }));
            edit.disabled = owner.state === TopicState.Archived;
            if (edit.disabled)
                edit.title = t("layout.archivedTopic");
            card.append(el("h4", subject.name), el("p", `${label("SubjectKind", subject.kind)} · ${t("research.versions", { p0: row.node_count })}`), el("code", subject.reference), actions(button(t("research.graph"), () => openGraph(owner, subject.id)), edit));
            get("subject-list").append(card);
        }
        if (!more && !page.subjects.length)
            get("subject-list").append(el("p", t("research.noSubjects")));
        subjectAfter = page.next_after;
        get("more-subjects").hidden = !subjectAfter;
    }
    catch (error) {
        if (request === subjectRequest)
            notice(error);
    }
}
export function initTopics() {
    get("new-topic").onclick = () => void topicForm(undefined, () => loadTopics()).catch(notice);
    get("topic-search-button").onclick = () => void loadTopics();
    get("more-topics").onclick = () => void loadTopics(true);
    get("more-subjects").onclick = () => void loadSubjects(true);
    get("close-topic").onclick = () => {
        selected = undefined;
        subjectRequest++;
        get("topic-detail").hidden = true;
    };
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
            void openGraph(selected).catch(notice);
        }
    };
}
async function openGraph(topic, subject = "") {
    await selectConversation(topic.group_id);
    sessionStorage.setItem("evolution-topic", topic.id);
    sessionStorage.setItem("evolution-subject", subject);
    revealResearch("evolution");
    await loadEvolution();
}

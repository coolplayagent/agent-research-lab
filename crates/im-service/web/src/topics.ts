import { revealResearch } from "./workspace.js";
import { loadEvolution } from "./evolution.js";
import { TopicState } from "/assets/shared/contracts.js";
import { api, query } from "./api.js";
import { get, el, button, notice } from "./dom.js";
import { t, label } from "./i18n.js";
import { icon } from "./icons.js";
import { openGroup as selectConversation } from "./conversation.js";
import { topicForm, subjectForm } from "./research-forms.js";
import type { Topic, TopicRow, SubjectRow } from "./research-types.js";
let after: string | null = null,
  selected: Topic | undefined,
  subjectAfter: string | null = null;
export async function loadTopics(more = false): Promise<void> {
  try {
    const page = await api<{ topics: TopicRow[]; next_after: string | null }>(
      "/api/im/topics?" +
        query({
          after: more ? after || "" : "",
          query: get<HTMLInputElement>("topic-search").value,
        }),
    );
    if (!more) get("topic-list").replaceChildren();
    for (const row of page.topics) {
      const card = el("article", undefined, "card"),
        topic = row.topic;
      const heading = el("header", undefined, "identity-heading");
      const symbol = el("span", undefined, "topic-symbol");
      symbol.append(icon("topic"));
      heading.append(symbol, el("h3", topic.title));
      card.append(
        heading,
        el("p", topic.objective),
        el(
          "p",
          `${label("TopicState", topic.state)} · ${t("research.counts", { p0: row.subject_count, p1: row.goal_count })}`,
        ),
        button(t("research.open"), () => openTopic(topic)),
        button(
          t("research.edit"),
          () =>
            void topicForm(topic, async () => {
              await loadTopics();
              if (selected?.id === topic.id) {
                const updated = (
                  await api<{ topics: TopicRow[] }>(
                    "/api/im/topics?" + query({ query: topic.title }),
                  )
                ).topics.find((r) => r.topic.id === topic.id)?.topic;
                if (updated) await openTopic(updated);
              }
            }),
        ),
      );
      get("topic-list").append(card);
    }
    if (!more && !page.topics.length)
      get("topic-list").append(el("p", t("research.noTopics")));
    after = page.next_after;
    get("more-topics").hidden = !after;
  } catch (error) {
    notice(error);
  }
}
async function openTopic(topic: Topic): Promise<void> {
  selected = topic;
  get("topic-detail").hidden = false;
  get("topic-detail-title").textContent = topic.title;
  get("topic-detail-objective").textContent = topic.objective;
  get<HTMLButtonElement>("new-subject").disabled =
    topic.state === TopicState.Archived;
  await loadSubjects();
}
async function loadSubjects(more = false): Promise<void> {
  if (!selected) return;
  try {
    const page = await api<{
      subjects: SubjectRow[];
      next_after: string | null;
    }>(
      "/api/im/subjects?" +
        query({ topic_id: selected.id, after: more ? subjectAfter || "" : "" }),
    );
    if (!more) get("subject-list").replaceChildren();
    for (const row of page.subjects) {
      const subject = row.subject,
        card = el("article", undefined, "card");
      card.append(
        el("h4", subject.name),
        el(
          "p",
          `${label("SubjectKind", subject.kind)} · ${t("research.versions", { p0: row.node_count })}`,
        ),
        el("code", subject.reference),
        button(t("research.graph"), () => {
          if (selected) void openGraph(selected, subject.id).catch(notice);
        }),
        button(t("research.edit"), () =>
          subjectForm(selected!, subject, () => loadSubjects()),
        ),
      );
      get("subject-list").append(card);
    }
    if (!more && !page.subjects.length)
      get("subject-list").append(el("p", t("research.noSubjects")));
    subjectAfter = page.next_after;
    get("more-subjects").hidden = !subjectAfter;
  } catch (error) {
    notice(error);
  }
}
export function initTopics(): void {
  get("new-topic").onclick = () =>
    void topicForm(undefined, () => loadTopics()).catch(notice);
  get("topic-search-button").onclick = () => void loadTopics();
  get("more-topics").onclick = () => void loadTopics(true);
  get("more-subjects").onclick = () => void loadSubjects(true);
  get("new-subject").onclick = () => {
    if (selected) subjectForm(selected, undefined, () => loadSubjects());
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

async function openGraph(topic: Topic, subject = ""): Promise<void> {
  await selectConversation(topic.group_id);
  sessionStorage.setItem("evolution-topic", topic.id);
  sessionStorage.setItem("evolution-subject", subject);
  revealResearch("evolution");
  await loadEvolution();
}

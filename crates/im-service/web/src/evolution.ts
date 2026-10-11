import { EvolutionState, TopicState } from "/assets/shared/contracts.js";
import { api, query } from "./api.js";
import { get, notice } from "./dom.js";
import { t } from "./i18n.js";
import { select as picker } from "./forms.js";
import { nodeForm } from "./research-forms.js";
import { renderGraph, showNode } from "./evolution-render.js";
import type {
  EvolutionRow,
  GraphPage,
  SubjectRow,
  TopicRow,
} from "./research-types.js";
let requestSequence = 0;
let groupId = "";
export function selectEvolution(id: string): void {
  if (id === groupId) return;
  groupId = id;
  requestSequence++;
  rows = [];
  topics = [];
  subjects = [];
  topic = "";
  subject = "";
  after = null;
  get("evolution-canvas").replaceChildren();
  get("evolution-filters").replaceChildren();
  get("evolution-detail").hidden = true;
  get("more-evolution").hidden = true;
  get("evolution-summary").textContent = t("workspace.loadingGraph");
  get<HTMLButtonElement>("new-evolution-node").disabled = true;
  if (!get("evolution-page").hidden) void loadEvolution();
}
let refreshTimer: ReturnType<typeof setTimeout> | undefined;
export function refreshEvolution(): void {
  if (
    get("evolution-page").hidden ||
    get("workspace").classList.contains("auxiliary")
  )
    return;
  if (refreshTimer) clearTimeout(refreshTimer);
  refreshTimer = setTimeout(() => {
    void loadEvolution();
  }, 120);
}
let rows: EvolutionRow[] = [],
  subjects: SubjectRow[] = [],
  topics: TopicRow[] = [],
  after: number | null = null,
  topic = "",
  subject = "",
  zoom = 1;
function updateCreate(): void {
  const owner = topics.find(row => row.topic.id === topic)?.topic;
  const button = get<HTMLButtonElement>("new-evolution-node");
  button.disabled = !subject || !owner || owner.state !== TopicState.Active;
  button.title = owner?.state === TopicState.Archived ? t("layout.archivedTopic") : "";
}
export async function loadEvolution(more = false): Promise<void> {
  const request = ++requestSequence,
    group = groupId;
  if (!group) {
    get("evolution-summary").textContent = t("workspace.selectGroup");
    return;
  }
  try {
    if (!more) {
      topic = sessionStorage.getItem("evolution-topic") || topic;
      subject = sessionStorage.getItem("evolution-subject") || subject;
      sessionStorage.removeItem("evolution-topic");
      sessionStorage.removeItem("evolution-subject");
      if (!(await filters(request, group))) return;
    }
    if (request !== requestSequence) return;
    if (!topic) {
      rows = [];
      after = null;
      renderGraph([], showNode);
      get("evolution-summary").textContent = t("workspace.noGroupTopics");
      get("more-evolution").hidden = true;
      get("evolution-detail").hidden = true;
      return;
    }
    await graph(more);
  } catch (error) {
    if (group === groupId) {
      get("evolution-summary").textContent = String(error);
      notice(error);
    }
  }
}
async function filters(request: number, group: string): Promise<boolean> {
  const loadedTopics: TopicRow[] = [],
    loadedSubjects: SubjectRow[] = [];
  let cursor = "",
    count = 0;
  do {
    const page = await api<{ topics: TopicRow[]; next_after: string | null }>(
      "/api/im/topics?" + query({ after: cursor }),
    );
    if (request !== requestSequence) return false;
    loadedTopics.push(
      ...page.topics.filter((row) => row.topic.group_id === group),
    );
    count += page.topics.length;
    const next = page.next_after || "";
    if (next && (next === cursor || count >= 8192))
      throw new Error(t("workspace.topicLimit"));
    cursor = next;
  } while (cursor);
  topics = loadedTopics;
  if (!topics.some((row) => row.topic.id === topic)) {
    topic = topics[0]?.topic.id || "";
    subject = "";
  }
  if (topic)
    do {
      const page = await api<{
        subjects: SubjectRow[];
        next_after: string | null;
      }>("/api/im/subjects?" + query({ topic_id: topic, after: cursor }));
      if (request !== requestSequence) return false;
      loadedSubjects.push(...page.subjects);
      const next = page.next_after || "";
      if (next && (next === cursor || loadedSubjects.length >= 8192))
        throw new Error(t("workspace.topicLimit"));
      cursor = next;
    } while (cursor);
  subjects = loadedSubjects;
  if (!subjects.some((row) => row.subject.id === subject)) subject = "";
  const topicPicker = picker(
      [...topics.map((r) => [r.topic.id, r.topic.title] as [string, string])],
      topic,
    ),
    subjectPicker = picker(
      [
        ["", t("research.allSubjects")],
        ...subjects.map(
          (r) => [r.subject.id, r.subject.name] as [string, string],
        ),
      ],
      subject,
    );
  topicPicker.setAttribute("aria-label", t("research.topics"));
  subjectPicker.setAttribute("aria-label", t("research.subjects"));
  get("evolution-filters").replaceChildren(topicPicker, subjectPicker);
  topicPicker.onchange = () => {
    topic = topicPicker.value;
    subject = "";
    void loadEvolution();
  };
  subjectPicker.onchange = () => {
    subject = subjectPicker.value;
    void graph(false).catch(notice);
  };
  updateCreate();
  return true;
}
async function graph(more: boolean): Promise<void> {
  if (!topic) return;
  const request = ++requestSequence;
  const selected = get("evolution-canvas")
    .querySelector('[aria-pressed="true"]')
    ?.getAttribute("data-node-id");
  const page = await api<GraphPage>(
    "/api/im/evolution?" +
      query({
        topic_id: topic,
        subject_id: subject,
        after: more ? after || 0 : 0,
      }),
  );
  if (request !== requestSequence) return;
  rows = more ? rows.concat(page.nodes) : page.nodes;
  after = page.next_after;
  get("more-evolution").hidden = !after;
  get("evolution-summary").textContent = t("research.graphCounts", {
    p0: rows.length,
    p1: new Set(rows.map((r) => r.subject.id)).size,
    p2: rows.filter((r) => r.status === EvolutionState.Accepted).length,
    p3: rows.filter((r) => r.status === EvolutionState.Failed).length,
  });
  updateCreate();
  get("evolution-detail").hidden = true;
  renderGraph(rows, showNode);
  const current = rows.find((row) => row.node.input.id === selected);
  if (current) {
    showNode(current);
    for (const node of get("evolution-canvas").querySelectorAll(
      "[data-node-id]",
    )) {
      if (node.getAttribute("data-node-id") === selected)
        node.setAttribute("aria-pressed", "true");
    }
  }
  scale();
}
function scale(): void {
  const svg = get("evolution-canvas").querySelector("svg");
  if (svg) {
    svg.style.width = svg.viewBox.baseVal.width * zoom + "px";
    svg.style.height = svg.viewBox.baseVal.height * zoom + "px";
  }
}
export function initEvolution(): void {
  get("refresh-evolution").onclick = () => void loadEvolution();
  get("more-evolution").onclick = () => void graph(true).catch(notice);
  get("evolution-zoom-in").onclick = () => {
    zoom = Math.min(1.5, zoom + 0.15);
    scale();
  };
  get("evolution-zoom-out").onclick = () => {
    zoom = Math.max(0.5, zoom - 0.15);
    scale();
  };
  get("new-evolution-node").onclick = () => {
    const target = subjects.find((r) => r.subject.id === subject)?.subject,
      owner = topics.find((r) => r.topic.id === target?.topic_id)?.topic;
    if (target && owner?.state === TopicState.Active)
      void nodeForm(target, owner.group_id, rows, () => graph(false)).catch(
        notice,
      );
  };
}

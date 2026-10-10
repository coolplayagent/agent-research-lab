import { EvolutionState } from "/assets/shared/contracts.js";
import { api, query } from "./api.js";
import { get, notice } from "./dom.js";
import { t } from "./i18n.js";
import { select as picker } from "./forms.js";
import { nodeForm } from "./research-forms.js";
import { renderGraph, showNode } from "./evolution-render.js";
let requestSequence = 0;
let refreshTimer;
export function refreshEvolution() {
    if (location.hash !== "#evolution")
        return;
    if (refreshTimer)
        clearTimeout(refreshTimer);
    refreshTimer = setTimeout(() => {
        void graph(false).catch(notice);
    }, 120);
}
let rows = [], subjects = [], topics = [], after = null, topic = "", subject = "", zoom = 1;
export async function loadEvolution(more = false) {
    try {
        if (!more) {
            topic = sessionStorage.getItem("evolution-topic") || "";
            subject = sessionStorage.getItem("evolution-subject") || "";
            sessionStorage.removeItem("evolution-topic");
            sessionStorage.removeItem("evolution-subject");
            await filters();
        }
        await graph(more);
    }
    catch (error) {
        notice(error);
    }
}
async function filters() {
    topics = [];
    subjects = [];
    let cursor = "";
    do {
        const page = await api("/api/im/topics?" + query({ after: cursor }));
        topics.push(...page.topics);
        cursor = page.next_after || "";
    } while (cursor && topics.length < 2048);
    do {
        const page = await api("/api/im/subjects?" + query({ topic_id: topic, after: cursor }));
        subjects.push(...page.subjects);
        cursor = page.next_after || "";
    } while (cursor && subjects.length < 8192);
    const topicPicker = picker([
        ["", t("research.allTopics")],
        ...topics.map((r) => [r.topic.id, r.topic.title]),
    ], topic), subjectPicker = picker([
        ["", t("research.allSubjects")],
        ...subjects.map((r) => [r.subject.id, r.subject.name]),
    ], subject);
    topicPicker.setAttribute("aria-label", t("research.topics"));
    subjectPicker.setAttribute("aria-label", t("research.subjects"));
    get("evolution-filters").replaceChildren(topicPicker, subjectPicker);
    topicPicker.onchange = () => {
        topic = topicPicker.value;
        subject = "";
        void filters()
            .then(() => graph(false))
            .catch(notice);
    };
    subjectPicker.onchange = () => {
        subject = subjectPicker.value;
        void graph(false).catch(notice);
    };
    get("new-evolution-node").disabled = !subject;
}
async function graph(more) {
    const request = ++requestSequence;
    const selected = get("evolution-canvas")
        .querySelector('[aria-pressed="true"]')
        ?.getAttribute("data-node-id");
    const page = await api("/api/im/evolution?" +
        query({
            topic_id: topic,
            subject_id: subject,
            after: more ? after || 0 : 0,
        }));
    if (request !== requestSequence)
        return;
    rows = more ? rows.concat(page.nodes) : page.nodes;
    after = page.next_after;
    get("more-evolution").hidden = !after;
    get("evolution-summary").textContent = t("research.graphCounts", {
        p0: rows.length,
        p1: new Set(rows.map((r) => r.subject.id)).size,
        p2: rows.filter((r) => r.status === EvolutionState.Accepted).length,
        p3: rows.filter((r) => r.status === EvolutionState.Failed).length,
    });
    get("new-evolution-node").disabled = !subject;
    get("evolution-detail").hidden = true;
    renderGraph(rows, showNode);
    const current = rows.find((row) => row.node.input.id === selected);
    if (current) {
        showNode(current);
        for (const node of get("evolution-canvas").querySelectorAll("[data-node-id]")) {
            if (node.getAttribute("data-node-id") === selected)
                node.setAttribute("aria-pressed", "true");
        }
    }
    scale();
}
function scale() {
    const svg = get("evolution-canvas").querySelector("svg");
    if (svg) {
        svg.style.width = svg.viewBox.baseVal.width * zoom + "px";
        svg.style.height = svg.viewBox.baseVal.height * zoom + "px";
    }
}
export function initEvolution() {
    get("refresh-evolution").onclick = () => void graph(false).catch(notice);
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
        const target = subjects.find((r) => r.subject.id === subject)?.subject, owner = topics.find((r) => r.topic.id === target?.topic_id)?.topic;
        if (target && owner)
            void nodeForm(target, owner.group_id, rows, () => graph(false)).catch(notice);
    };
}

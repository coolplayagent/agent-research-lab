import { revealResearch } from "./workspace.js";
import { GoalState, GroupKind, WorkState } from "/assets/shared/contracts.js";
import { t } from "./i18n.js";
import { label } from "./i18n.js";
import { api, query } from "./api.js";
import { button, el, get, notice } from "./dom.js";
import { display } from "./people.js";
let selected = null;
let snapshot = [];
export function goalSnapshot() {
    return snapshot;
}
export function suspendGoals() {
    selected = null;
    generation++;
    window.clearTimeout(expiryTimer);
    snapshot = [];
    get("goal-list").replaceChildren();
    get("more-goals").hidden = true;
    get("goal-summary").textContent = t("workspace.goalSummary");
}
export async function focusGoal(id) {
    const groupId = selected?.id;
    revealResearch("goals");
    while (selected?.id === groupId && !snapshot.some((goal) => goal.input.id === id) && next) {
        const cursor = next;
        await loadGoals(true);
        if (next === cursor)
            break;
    }
    if (selected?.id !== groupId)
        return;
    for (const node of get("goal-list").querySelectorAll("[data-goal-id]")) {
        if (node.dataset.goalId === id) {
            node.open = true;
            node.scrollIntoView({ block: "nearest" });
            return;
        }
    }
    notice(t("goals.notFound"));
}
let generation = 0;
let next = null;
let expiryTimer;
export function selectGoals(group) {
    suspendGoals();
    selected = group;
    get("goal-section").hidden = group.kind === GroupKind.Board;
    get("new-goal").disabled = group.archived;
    void loadGoals();
}
async function mutate(goal, operation, work_id) {
    try {
        await api("/api/im/goals", {
            operation,
            goal_id: goal.input.id,
            revision: goal.revision,
            ...(work_id ? { work_id } : {}),
        });
        await loadGoals();
    }
    catch (error) {
        notice(error);
    }
}
function expired(work) {
    return (work.state === WorkState.Running &&
        work.deadline_ms !== null &&
        work.deadline_ms <= Date.now());
}
function card(goal) {
    const details = el("details", undefined, "goal-card");
    details.dataset.goalId = goal.input.id;
    const summary = el("summary");
    summary.append(el("strong", goal.input.title), el("span", label("GoalState", goal.state), "badge"));
    details.append(summary, el("p", goal.input.objective), el("p", t("goals.1459c102bb") + goal.input.acceptance), el("small", t("goals.6ea45dffbd", {
        p0: goal.input.scenario === "research"
            ? t("goals.70596ea10b")
            : t("goals.0ad442f5f7"),
        p1: goal.input.max_seconds,
    })));
    for (const work of goal.work) {
        const row = el("section", undefined, "work-item");
        row.append(el("strong", display(work.person_id)), el("span", expired(work)
            ? t("goals.58ada57084")
            : goal.state === GoalState.Completed &&
                work.state === WorkState.Submitted
                ? t("goals.788add54b2")
                : label("WorkState", work.state), "badge"), el("p", work.instruction));
        if (work.result) {
            row.append(el("p", goal.input.scenario === "research" && work.result.code
                ? label("WorkResultCode", work.result.code)
                : work.result.summary));
            const evidence = el("details");
            evidence.dataset.evidenceKey = `${goal.input.id}/${work.id}/${work.attempt}`;
            evidence.append(el("summary", t("goals.da8098216f")), el("pre", JSON.stringify(work.result.evidence, null, 2)));
            row.append(evidence);
        }
        if (goal.state === GoalState.Active &&
            (expired(work) ||
                work.state === WorkState.Submitted ||
                work.state === WorkState.Failed))
            row.append(button(t("goals.bd7dd17c34"), () => mutate(goal, "retry", work.id)));
        details.append(row);
    }
    if (goal.state === GoalState.Active) {
        const actions = el("div", undefined, "actions");
        const accept = button(t("goals.9a341c73e2"), () => mutate(goal, "accept"));
        accept.disabled = goal.work.some((w) => w.state !== WorkState.Submitted);
        const cancel = button(t("goals.a53a4e7bca"), () => mutate(goal, "cancel"));
        cancel.disabled = goal.work.some((w) => w.state === WorkState.Running && !expired(w));
        actions.append(accept, cancel);
        details.append(actions);
    }
    return details;
}
export async function loadGoals(append = false) {
    if (!selected || selected.kind === GroupKind.Board)
        return;
    const groupId = selected.id, request = ++generation;
    try {
        const loaded = [];
        let cursor = append ? next || "" : "";
        const target = append ? 0 : snapshot.length;
        do {
            const page = await api("/api/im/goals?" +
                query({ group_id: groupId, after: cursor }));
            if (generation !== request || selected?.id !== groupId)
                return;
            loaded.push(...page.goals);
            const following = page.next_after || "";
            if (following === cursor) {
                cursor = "";
                break;
            }
            cursor = following;
        } while (!append && cursor && loaded.length < target);
        snapshot = append ? [...snapshot, ...loaded] : loaded;
        window.clearTimeout(expiryTimer);
        const deadlines = snapshot.flatMap((g) => g.work
            .filter((w) => w.state === WorkState.Running &&
            w.deadline_ms !== null &&
            w.deadline_ms > Date.now())
            .map((w) => w.deadline_ms));
        if (deadlines.length)
            expiryTimer = window.setTimeout(() => void loadGoals(), Math.max(1, Math.min(...deadlines) - Date.now() + 50));
        const active = snapshot.find((goal) => goal.state === GoalState.Active);
        get("goal-summary").textContent = active
            ? t("workspace.currentGoal", { title: active.input.title })
            : t("workspace.goalSummary");
        document.dispatchEvent(new Event("workspace-goals"));
        const list = get("goal-list");
        const opened = new Set([
            ...list.querySelectorAll("details[open][data-goal-id]"),
        ].map((d) => d.dataset.goalId));
        const evidenceOpened = new Set([...list.querySelectorAll("details[open][data-evidence-key]")]
            .map((node) => node.dataset.evidenceKey));
        if (!append)
            list.replaceChildren();
        for (const goal of loaded) {
            const node = card(goal);
            node.open = opened.has(goal.input.id);
            for (const evidence of node.querySelectorAll("[data-evidence-key]"))
                evidence.open = evidenceOpened.has(evidence.dataset.evidenceKey);
            list.append(node);
        }
        if (!list.childElementCount)
            list.append(el("p", t("goals.1bc752e12f"), "goal-empty"));
        next = cursor || null;
        get("more-goals").hidden = !next;
        get("more-goals").onclick = () => void loadGoals(true);
    }
    catch (error) {
        if (generation === request)
            notice(error);
    }
}

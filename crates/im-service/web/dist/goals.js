import { api, query } from "./api.js";
import { button, el, get, notice } from "./dom.js";
import { display } from "./people.js";
let selected = null;
let generation = 0;
let next = null;
let expiryTimer;
const states = {
    active: "协作中",
    completed: "已验收",
    cancelled: "已取消",
    ready: "等待领取",
    running: "执行中",
    submitted: "已提交 · 待验收",
    failed: "执行失败",
};
export function selectGoals(group) {
    window.clearTimeout(expiryTimer);
    selected = group;
    get("goal-section").hidden = group.kind === "board";
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
    return (work.state === "running" &&
        work.deadline_ms !== null &&
        work.deadline_ms <= Date.now());
}
function card(goal) {
    const details = el("details", undefined, "goal-card");
    details.dataset.goalId = goal.input.id;
    const summary = el("summary");
    summary.append(el("strong", goal.input.title), el("span", states[goal.state], "badge"));
    details.append(summary, el("p", goal.input.objective), el("p", "验收标准：" + goal.input.acceptance), el("small", `${goal.input.scenario === "research" ? "CLI 研究" : "通用协作"} · 每项任务限时 ${goal.input.max_seconds} 秒`));
    for (const work of goal.work) {
        const row = el("section", undefined, "work-item");
        row.append(el("strong", display(work.person_id)), el("span", expired(work)
            ? "执行时限已到 · 可重试"
            : goal.state === "completed" && work.state === "submitted"
                ? "结果已验收"
                : states[work.state], "badge"), el("p", work.instruction));
        if (work.result) {
            row.append(el("p", work.result.summary));
            const evidence = el("details");
            evidence.append(el("summary", "查看结果证据"), el("pre", JSON.stringify(work.result.evidence, null, 2)));
            row.append(evidence);
        }
        if (goal.state === "active" &&
            (expired(work) || ["submitted", "failed"].includes(work.state)))
            row.append(button("退回重做", () => void mutate(goal, "retry", work.id)));
        details.append(row);
    }
    if (goal.state === "active") {
        const actions = el("div", undefined, "actions");
        const accept = button("验收目标", () => void mutate(goal, "accept"));
        accept.disabled = goal.work.some((w) => w.state !== "submitted");
        const cancel = button("取消目标", () => void mutate(goal, "cancel"));
        cancel.disabled = goal.work.some((w) => w.state === "running" && !expired(w));
        actions.append(accept, cancel);
        details.append(actions);
    }
    return details;
}
export async function loadGoals(append = false) {
    if (!selected || selected.kind === "board")
        return;
    const groupId = selected.id, request = ++generation;
    try {
        const page = await api("/api/im/goals?" +
            query({ group_id: groupId, after: append ? next || "" : "" }));
        if (generation !== request || selected.id !== groupId)
            return;
        window.clearTimeout(expiryTimer);
        const deadlines = page.goals.flatMap((g) => g.work
            .filter((w) => w.state === "running" &&
            w.deadline_ms !== null &&
            w.deadline_ms > Date.now())
            .map((w) => w.deadline_ms));
        if (deadlines.length)
            expiryTimer = window.setTimeout(() => void loadGoals(), Math.max(1, Math.min(...deadlines) - Date.now() + 50));
        const list = get("goal-list");
        const opened = new Set([
            ...list.querySelectorAll("details[open][data-goal-id]"),
        ].map((d) => d.dataset.goalId));
        if (!append)
            list.replaceChildren();
        for (const goal of page.goals) {
            const node = card(goal);
            node.open = opened.has(goal.input.id);
            list.append(node);
        }
        if (!list.childElementCount)
            list.append(el("p", "为群组设置目标，给成员分工，在这里汇集进度与结果。", "goal-empty"));
        next = page.next_after;
        get("more-goals").hidden = !next;
        get("more-goals").onclick = () => void loadGoals(true);
    }
    catch (error) {
        if (generation === request)
            notice(error);
    }
}

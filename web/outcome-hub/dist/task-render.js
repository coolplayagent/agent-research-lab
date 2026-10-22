import { TaskKind, TaskState, TaskNodeState, TaskErrorCode } from "/assets/shared/contracts.js";
import { el, button, actions } from "./dom.js";
import { t, label } from "./i18n.js";
import { outcomeLink } from "./outcome-detail.js";
import { display } from "./people.js";
export function taskCard(task, select) {
    const card = el("article", undefined, "card task-card");
    const heading = button(task.title, select);
    heading.className = "task-title";
    card.append(heading, el("span", label("TaskState", task.state), "task-state"), el("p", task.input.intent), el("small", t("tasks.progress", { done: task.nodes.filter(n => n.state === TaskNodeState.Approved).length, total: task.nodes.length })));
    if (task.question)
        card.append(el("p", task.question, "task-attention"));
    if (task.error)
        card.append(el("p", label("TaskErrorCode", task.error), "task-attention"));
    const delivery = task.deliveries.at(-1);
    if (delivery)
        card.append(el("p", delivery.summary.slice(0, 220), "task-preview"));
    return card;
}
export function taskGraph(task, choose) {
    const graph = el("div", undefined, "task-graph");
    if (!task.nodes.length) {
        graph.append(el("p", t("tasks.planning")));
        return graph;
    }
    const depths = new Map();
    for (let i = 0; i < task.nodes.length; i++)
        for (const n of task.nodes)
            if (!depths.has(n.input.id) && n.input.dependencies.every(d => depths.has(d)))
                depths.set(n.input.id, n.input.dependencies.length ? 1 + Math.max(...n.input.dependencies.map(d => depths.get(d))) : 0);
    for (let layer = 0; layer <= Math.max(0, ...depths.values()); layer++) {
        const column = el("section", undefined, "task-layer");
        column.append(el("h3", t("tasks.stage", { stage: layer + 1 })));
        for (const n of task.nodes.filter(n => depths.get(n.input.id) === layer)) {
            const card = el("article", undefined, "task-node");
            card.dataset.node = n.input.id;
            const title = button(n.input.title, () => choose(n.input.id));
            title.className = "task-title";
            card.append(title, el("p", display(n.input.person_id)), el("span", label("TaskNodeState", n.state), "task-state"), el("p", n.input.deliverable));
            if (n.input.dependencies.length) {
                const deps = el("div", undefined, "task-dependencies");
                deps.append(el("small", t("tasks.dependsOn")));
                for (const id of n.input.dependencies)
                    deps.append(button("← " + (task.nodes.find(x => x.input.id === id)?.input.title || id), () => choose(id)));
                card.append(deps);
            }
            column.append(card);
        }
        graph.append(column);
    }
    return graph;
}
export function nodeDetail(task, node) {
    const detail = el("section", undefined, "task-node-detail");
    detail.append(el("h3", node.input.title), el("p", t("tasks.owner", { name: display(node.input.person_id) })), el("p", node.input.instruction), el("h4", t("tasks.expected")), el("p", node.input.deliverable), el("h4", t("tasks.acceptance")), el("p", node.input.acceptance));
    if (node.review)
        detail.append(el("h4", t("tasks.coordinatorReview")), el("p", node.review));
    const artifact = task.artifacts.find(a => a.id === node.artifact_id);
    if (artifact) {
        detail.append(el("h4", t("tasks.output")), el("div", artifact.summary, "task-prose"), el("small", t("tasks.unverified")));
        if (task.kind === TaskKind.Outcome)
            detail.append(outcomeLink(artifact.id, t("tasks.openEvidence")));
        const evidence = el("details");
        evidence.append(el("summary", t("tasks.evidence")), el("pre", JSON.stringify(artifact.evidence, null, 2)));
        detail.append(evidence);
    }
    for (const id of node.input.dependencies) {
        const upstream = task.nodes.find(n => n.input.id === id), result = task.artifacts.find(a => a.id === upstream?.artifact_id);
        if (result) {
            const section = el("details");
            section.append(el("summary", t("tasks.upstream", { title: upstream.input.title })), el("div", result.summary, "task-prose"));
            detail.append(section);
        }
    }
    detail.append(attemptLogs(node.attempts, "tasks.logs"));
    return detail;
}
function attemptLogs(runs, title) {
    const logs = el("details");
    logs.append(el("summary", t(title)));
    if (!runs.length)
        logs.append(el("p", t("tasks.noLogs")));
    for (const run of runs) {
        const attempt = el("section");
        attempt.append(el("h4", t("tasks.attempt", { attempt: run.attempt })), el("p", new Date(run.started_ms).toLocaleString()), el("p", run.error ? label("TaskErrorCode", run.error) : run.finished_ms ? t("tasks.attemptFinished") : t("tasks.attemptRunning")), el("pre", JSON.stringify(run.receipt, null, 2)));
        logs.append(attempt);
    }
    return logs;
}
export function taskContent(task, change, chooseNode, selectedNode) {
    const root = el("div"), head = el("header", undefined, "task-heading"), status = el("div");
    status.append(el("h2", task.title), el("span", label("TaskState", task.state), "task-state"));
    head.append(status, actions(...(task.state === TaskState.Paused ? [button(t("tasks.resume"), () => change({ operation: "resume" }))] : []), button(t("tasks.back"), () => { location.hash = "tasks"; }), ...(task.state === TaskState.Review ? [button(t("tasks.accept"), () => change({ operation: "accept" }))] : []), ...(![TaskState.Completed, TaskState.Cancelled].includes(task.state) ? [button(t("tasks.cancel"), () => change({ operation: "cancel" }))] : [])));
    root.append(head);
    root.append(el("p", task.input.intent, "task-prose"));
    if (task.acceptance)
        root.append(el("p", t("tasks.criteria", { text: task.acceptance })));
    root.append(el("small", t("tasks.budgetSummary", { minutes: Math.ceil(task.active_ms / 60000), limit: Math.ceil(task.input.budget.active_seconds / 60), revisions: task.revisions_used, maxRevisions: task.input.budget.max_revisions })));
    if (task.question)
        root.append(el("p", task.question, "task-attention"));
    if (task.error)
        root.append(el("p", label("TaskErrorCode", task.error), "task-attention"));
    if (task.kind === TaskKind.Outcome) {
        const delivery = task.deliveries.at(-1), outcomes = el("section", undefined, "task-delivery");
        outcomes.append(el("h3", t("tasks.delivery")));
        if (delivery) {
            outcomes.append(el("div", delivery.summary, "task-prose"));
            if (delivery.return_reason)
                outcomes.append(el("p", t("tasks.returnedReason", { text: delivery.return_reason })));
        }
        else
            outcomes.append(el("p", t("tasks.deliveryPending")));
        outcomes.append(el("small", t("tasks.acceptanceNote")));
        root.append(outcomes);
    }
    root.append(el("h3", t(task.kind === TaskKind.Outcome ? "tasks.graph" : "tasks.directQuestion")), taskGraph(task, chooseNode));
    const n = task.nodes.find(n => n.input.id === selectedNode);
    if (n)
        root.append(nodeDetail(task, n));
    if (task.kind === TaskKind.Outcome) {
        root.append(attemptLogs(task.coordinator_attempts, "tasks.coordinatorLogs"));
        const graphs = el("details");
        graphs.append(el("summary", t("tasks.graphHistory")));
        task.graph_history.forEach((nodes, i) => { graphs.append(el("h4", t("tasks.graphRevision", { revision: i + 1 }))); for (const n of nodes)
            graphs.append(el("p", n.title + " · " + display(n.person_id) + (n.dependencies.length ? " ← " + n.dependencies.join(", ") : ""))); });
        root.append(graphs);
    }
    const history = el("details");
    history.append(el("summary", t("tasks.activity")));
    const list = el("ol");
    for (const event of task.events) {
        const item = el("li");
        item.append(el("time", new Date(event.created_ms).toLocaleTimeString()), el("strong", label("TaskEventKind", event.kind)), el("span", event.node_id ? task.nodes.find(n => n.input.id === event.node_id)?.input.title || event.node_id : ""), el("p", Object.values(TaskErrorCode).includes(event.text) ? label("TaskErrorCode", event.text) : event.text));
        list.append(item);
    }
    history.append(list);
    root.append(history);
    return root;
}

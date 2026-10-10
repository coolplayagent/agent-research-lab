import { workspaceState } from "./state.js";
import { jobs, cohortJobs, name, roleLabel, presenceLabel, agentPresence, avatar, choose, } from "./identity.js";
import { rooms, privateChats, timeline } from "./channels.js";
import { $, el } from "./ui.js";
import { roomName, renderSystemMessages } from "./navigation.js";
import { openPerson } from "./people-profile.js";
import { renderPeopleSidebar, syncPeople } from "./people-directory.js";
import { graph } from "./graph.js";
import { details } from "./profile.js";
import { renderSession, fetchSession } from "./sessions.js";
import { observeLineage } from "./lineage.js";
export function render() {
    if (!workspaceState.data)
        return;
    const list = jobs(), all = cohortJobs();
    if (!list.some((j) => j.id === workspaceState.selected))
        workspaceState.selected = list[0]?.id || "";
    rooms();
    privateChats();
    $("room-title").textContent = workspaceState.room
        ? "# " + roomName(workspaceState.room)
        : "# 全部研究记录";
    $("room-subtitle").textContent =
        `${all.length} 个任务 · ${workspaceState.data.max_agents || 8} 个并发上限 · SuperPOD 公共知识库`;
    const count = (states) => all.filter((j) => states.includes(j.state)).length;
    $("running").textContent = count(["running"]);
    $("pending").textContent = count([
        "ready",
        "waiting_dependencies",
        "retry_wait",
    ]);
    $("succeeded").textContent = count(["succeeded"]);
    $("blocked").textContent = count([
        "blocked",
        "failed",
        "needs_reconciliation",
        "unknown",
    ]);
    $("history-count").textContent =
        `全库 ${workspaceState.data.jobs.length} 个任务 · 历史阻塞 ${workspaceState.data.summary?.blocked || 0} 个`;
    const control = workspaceState.data.controller || {}, phases = {
        starting: "启动中",
        recovering: "恢复历史任务",
        refreshing: "核对最新版本",
        idle: "待命",
        running: "研究运行中",
        stopped: "已停止",
        unknown: "状态未上报",
    };
    $("controller-status").textContent = control.live
        ? phases[control.phase] || control.phase
        : "控制器离线";
    $("setting-follow").checked = workspaceState.following;
    $("setting-motion").checked = workspaceState.preferences.motion !== false;
    const info = $("service-settings-info");
    info.replaceChildren();
    for (const [label, value] of [
        ["服务状态", $("controller-status").textContent],
        ["并发上限", `${workspaceState.data.max_agents || 8} 个数字人`],
        ["当前任务", `${workspaceState.data.jobs.length} 个`],
        ["知识库", "SuperPOD"],
    ])
        info.append(el("dt", label), el("dd", value));
    const rows = $("tasks");
    const scroll = rows.scrollTop;
    rows.replaceChildren();
    const listedPeople = new Set();
    list.forEach((j) => {
        const personId = j.profile?.person_id || j.id;
        if (listedPeople.has(personId))
            return;
        listedPeople.add(personId);
        const b = el("button", undefined, `task-row ${j.id === workspaceState.selected ? "selected" : ""}`), text = el("span", name(j), "task-name");
        text.append(el("small", `${roleLabel(j)} · ${j.model} · ${presenceLabel(j)}`));
        const dot = el("span", undefined, `presence ${agentPresence(j).state}`);
        dot.title = presenceLabel(j);
        b.append(avatar(j), text, dot);
        b.title = j.id;
        b.addEventListener("click", () => j.profile?.person_id
            ? openPerson(j.profile.person_id)
            : choose(j.id, true));
        rows.append(b);
    });
    rows.scrollTop = scroll;
    $("task-count").textContent = listedPeople.size;
    if (typeof renderPeopleSidebar === "function")
        renderPeopleSidebar(list);
    {
        graph(list);
        details(workspaceState.data.jobs.find((j) => j.id === (workspaceState.inspected || workspaceState.selected)));
        timeline(list);
    }
    renderSession();
    connection();
    observeLineage(valueLineageRevision());
}
export function valueLineageRevision() {
    return workspaceState.data?.lineage_revision ?? null;
}
export function accept(value) {
    workspaceState.snapshotError = "";
    value.jobs = value.jobs || [];
    value.boards = value.boards || [];
    value.sessions = value.sessions || [];
    if (typeof syncPeople === "function")
        syncPeople(value.roster);
    const knownRuns = new Set(value.jobs.map((j) => j.run_id));
    for (const id of workspaceState.profiles.keys())
        if (!knownRuns.has(id))
            workspaceState.profiles.delete(id);
    let sessionChanged = false;
    for (const s of value.sessions) {
        const old = workspaceState.lastBytes.get(s.run_id);
        if (old !== undefined && old !== s.bytes)
            workspaceState.pulses.set(s.run_id, Date.now() + 1600);
        if (old !== s.bytes && workspaceState.historyState?.run === s.run_id)
            sessionChanged = true;
        workspaceState.lastBytes.set(s.run_id, s.bytes);
    }
    if (workspaceState.data) {
        const known = new Set(workspaceState.data.boards.flatMap((b) => (b.retained_messages || []).map((m) => m.message.id)));
        for (const b of value.boards)
            for (const m of b.retained_messages || [])
                if (!known.has(m.message.id))
                    workspaceState.publishPulses.set(m.message.task_id, Date.now() + 2000);
    }
    workspaceState.data = value;
    if (!workspaceState.data.jobs.some((j) => j.id === workspaceState.inspected))
        workspaceState.inspected = "";
    if (workspaceState.following) {
        const active = workspaceState.data.jobs.find((j) => j.state === "running" && j.cohort_id), seed = workspaceState.data.jobs.find((j) => (workspaceState.data.controller?.seed?.queued || []).includes(j.id) &&
            j.cohort_id);
        workspaceState.room =
            active?.cohort_id ||
                seed?.cohort_id ||
                workspaceState.data.jobs.find((j) => j.cohort_id)?.cohort_id ||
                "";
    }
    render();
    if (sessionChanged &&
        workspaceState.historyState?.following &&
        $("session-window").open)
        fetchSession();
}
export function connection() {
    renderSystemMessages();
    const age = workspaceState.data?.sampled_at
        ? Math.max(0, Math.floor(Date.now() / 1000 - workspaceState.data.sampled_at))
        : null;
    const healthy = workspaceState.connected &&
        !!workspaceState.data &&
        !workspaceState.data.error;
    $("connection-dot").classList.toggle("live", healthy);
    $("connection").textContent = healthy
        ? "实时连接"
        : workspaceState.connected
            ? "连接已建立"
            : "正在重连";
    $("sample-time").textContent =
        age === null ? "等待快照" : `最近变化 ${age} 秒前 · 事件推送`;
}
export async function refresh() {
    if (workspaceState.fetching)
        return;
    workspaceState.fetching = true;
    $("refresh").disabled = true;
    try {
        const r = await fetch("/api/snapshot", {
            cache: "no-store",
            signal: AbortSignal.timeout(8000),
        });
        if (!r.ok)
            throw new Error("snapshot");
        workspaceState.snapshotError = "";
        accept(await r.json());
    }
    catch (_) {
        workspaceState.snapshotError = "无法获取运行快照，正在自动重试。";
        workspaceState.connected = false;
        connection();
    }
    finally {
        workspaceState.fetching = false;
        $("refresh").disabled = false;
    }
}
export function connect() {
    workspaceState.stream = new EventSource("/api/events");
    workspaceState.stream.addEventListener("snapshot", (e) => {
        try {
            workspaceState.connected = true;
            accept(JSON.parse(e.data));
        }
        catch (_) {
            workspaceState.connected = false;
            connection();
        }
    });
    workspaceState.stream.onopen = () => {
        workspaceState.connected = true;
        connection();
    };
    workspaceState.stream.onerror = () => {
        workspaceState.connected = false;
        connection();
    };
}

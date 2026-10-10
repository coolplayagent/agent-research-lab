import { workspaceState } from "./state.js";
import { agentPresence, avatar } from "./identity.js";
import { renderExecutorDirectory } from "./executors.js";
import { $, el } from "./ui.js";
import { refresh } from "./observer.js";
import { openPerson } from "./people-profile.js";
export function digitalPersonId(j) {
    return j?.profile?.person_id || j?.id;
}
export function currentPersonJob(id) {
    const list = (workspaceState.data?.jobs || []).filter((j) => digitalPersonId(j) === id);
    return (list.find((j) => j.state === "running") ||
        list.find((j) => j.cohort_id === workspaceState.room) ||
        list[0]);
}
export function personPresence(id) {
    const list = (workspaceState.data?.jobs || []).filter((j) => digitalPersonId(j) === id);
    const priority = { offline: 0, online: 1, chatting: 2, busy: 3 };
    return (list
        .map(agentPresence)
        .sort((a, b) => priority[b.state] - priority[a.state])[0]
        ?.state || "offline");
}
export function personRepresentative(person) {
    return (currentPersonJob(person.id) || {
        id: person.id,
        role: person.role,
        model: person.execution?.model || "按任务选择",
        backend: person.execution?.backend || "按任务选择",
        state: "offline",
        profile: {
            person_id: person.id,
            display_name: person.name,
            handle: person.id,
        },
        dependencies: [],
    });
}
export function syncPeople(roster) {
    if (!roster)
        return;
    workspaceState.peopleTotal = roster.total || workspaceState.peopleTotal;
    if ((roster.revision || 0) >= workspaceState.peopleRevision) {
        workspaceState.peopleDefaults = roster.defaults || {};
        workspaceState.peopleModels = roster.models || {};
        workspaceState.executionCatalog =
            roster.execution_catalog || workspaceState.executionCatalog;
        renderExecutorDirectory();
        workspaceState.peopleRevision = roster.revision || 0;
    }
    for (const p of roster.people || []) {
        if (!workspaceState.peopleIndex.has(p.id) ||
            workspaceState.peopleIndex.get(p.id).revision <= p.revision)
            workspaceState.peopleIndex.set(p.id, p);
    }
    renderPeopleDirectory();
    $("people-more").hidden =
        !workspaceState.peopleCursor ||
            workspaceState.peopleIndex.size >= workspaceState.peopleTotal;
}
export async function peopleApi(body) {
    const response = await fetch("/api/people", {
        method: "POST",
        headers: {
            "Content-Type": "application/json",
            "X-Crystal-Intent": "manage-people",
        },
        body: JSON.stringify(body),
        signal: AbortSignal.timeout(25000),
    });
    const value = await response.json();
    if (!response.ok)
        throw new Error(value.error || "请求未完成，请稍后重试。");
    return value;
}
export async function loadPeople(after = "") {
    if (workspaceState.directoryLoading)
        return;
    workspaceState.directoryLoading = true;
    $("people-more").disabled = true;
    try {
        const response = await fetch(`/api/people${after ? `?after=${encodeURIComponent(after)}` : ""}`, { signal: AbortSignal.timeout(8000) });
        if (!response.ok)
            throw new Error("名册暂不可读。");
        const value = await response.json();
        workspaceState.peopleCursor = value.next_after;
        syncPeople(value);
        $("people-settings-status").textContent =
            `共 ${value.total} 位数字人，已加载 ${workspaceState.peopleIndex.size} 位。`;
    }
    catch (error) {
        $("people-settings-status").textContent = error.message;
    }
    finally {
        workspaceState.directoryLoading = false;
        $("people-more").disabled = false;
        $("people-more").hidden =
            !workspaceState.peopleCursor ||
                workspaceState.peopleIndex.size >= workspaceState.peopleTotal;
    }
}
export function renderPeopleDirectory() {
    const key = JSON.stringify([
        Array.from(workspaceState.peopleIndex.values()),
        workspaceState.peopleDefaults,
        workspaceState.peopleModels,
        $("people-search").value,
        $("people-temporary").checked,
    ]);
    if (key === workspaceState.peopleKey)
        return;
    workspaceState.peopleKey = key;
    const defaults = $("people-defaults");
    defaults.replaceChildren();
    for (const [role, id] of Object.entries(workspaceState.peopleDefaults)) {
        const label = el("label", `${workspaceState.personRoles[role] || role} · 默认成员`), select = el("select");
        select.setAttribute("aria-label", `${workspaceState.personRoles[role] || role}默认成员`);
        for (const p of workspaceState.peopleIndex.values())
            if (p.kind === "fixed") {
                const option = el("option", p.name);
                option.value = p.id;
                select.append(option);
            }
        select.value = id;
        select.addEventListener("change", async () => {
            select.disabled = true;
            try {
                await peopleApi({
                    operation: "profile",
                    change: { action: "set_default", role, id: select.value },
                });
                workspaceState.peopleDefaults[role] = select.value;
                $("people-settings-status").textContent =
                    "默认成员已保存，新的研究任务会复用它。";
                await loadPeople();
                await refresh();
            }
            catch (error) {
                select.value = id;
                $("people-settings-status").textContent = error.message;
            }
            finally {
                select.disabled = false;
            }
        });
        label.append(select);
        defaults.append(label);
    }
    const root = $("people-directory"), query = $("people-search").value.toLowerCase();
    root.replaceChildren();
    const list = Array.from(workspaceState.peopleIndex.values())
        .filter((p) => ($("people-temporary").checked || p.kind !== "temporary") &&
        [p.name, p.role, workspaceState.personRoles[p.role], p.purpose]
            .join(" ")
            .toLowerCase()
            .includes(query))
        .sort((a, b) => b.created_at - a.created_at || a.name.localeCompare(b.name));
    for (const p of list) {
        const card = el("button", undefined, "person-card"), text = el("span", p.name, "task-name");
        text.append(el("small", `${workspaceState.personKinds[p.kind]} · ${workspaceState.personRoles[p.role] || p.role} · ${p.task_count || 0} 个任务`));
        card.append(avatar(personRepresentative(p)), text);
        card.addEventListener("click", () => openPerson(p.id));
        root.append(card);
    }
    if (!list.length)
        root.append(el("p", "没有匹配的数字人。可以新增成员，或显示临时成员。", "empty"));
}
export function settingsPeopleView(view) {
    $("people-list").hidden = view !== "list";
    $("person-form").hidden = view !== "form";
    $("person-inline-host").hidden = view !== "detail";
}
export function closePerson() {
    const panel = $("person-window");
    panel.close();
    workspaceState.personRequest++;
    if (panel.classList.contains("embedded")) {
        settingsPeopleView("list");
        panel.classList.remove("embedded");
        document.body.append(panel);
    }
}
export function renderPeopleSidebar(list) {
    if (!workspaceState.data?.roster)
        return;
    const root = $("tasks"), scroll = root.scrollTop, query = $("search").value.toLowerCase().replace(/^@/, "");
    const current = new Set(list.map(digitalPersonId));
    const filtered = Array.from(workspaceState.peopleIndex.values()).filter((p) => (p.kind !== "temporary" || current.has(p.id)) &&
        [
            p.id,
            p.name,
            p.role,
            p.purpose,
            workspaceState.personRoles[p.role],
            workspaceState.peopleModels[p.role],
        ]
            .join(" ")
            .toLowerCase()
            .includes(query));
    const priority = { offline: 0, online: 1, chatting: 2, busy: 3 };
    filtered.sort((a, b) => priority[personPresence(b.id)] - priority[personPresence(a.id)] ||
        a.name.localeCompare(b.name));
    root.replaceChildren();
    for (const p of filtered) {
        const j = personRepresentative(p), state = personPresence(p.id);
        if ($("state").value && j.state !== $("state").value)
            continue;
        const row = el("button", undefined, `task-row ${digitalPersonId(workspaceState.data.jobs.find((j) => j.id === workspaceState.selected)) === p.id ? "selected" : ""}`), text = el("span", p.name, "task-name");
        text.append(el("small", `${workspaceState.personRoles[p.role] || p.role} · ${{ offline: "离线", online: "在线", busy: "忙碌", chatting: "对话中" }[state]} · ${p.task_count || 0} 个任务`));
        row.append(avatar(j), text, el("span", undefined, `presence ${state}`));
        row.title = `${workspaceState.personKinds[p.kind]} · @${p.name}`;
        row.addEventListener("click", () => openPerson(p.id));
        root.append(row);
    }
    root.scrollTop = scroll;
    $("task-count").textContent = root.children.length;
}

import { workspaceState } from "./state.js";
import { closePerson, settingsPeopleView, loadPeople, personPresence, personRepresentative, peopleApi, } from "./people-directory.js";
import { showPage } from "./navigation.js";
import { $, el, badge, messageTimestamp } from "./ui.js";
import { closeSession, openSessionRecord } from "./sessions.js";
import { avatar, readable, fold } from "./identity.js";
export function editPerson(person = null) {
    closePerson();
    location.hash = "settings/people";
    showPage("settings/people", true);
    workspaceState.editingPerson = person;
    settingsPeopleView("form");
    $("person-form-title").textContent = person
        ? `编辑 ${person.name}`
        : "新增数字人";
    $("person-name").value = person?.name || "";
    $("person-purpose").value = person?.purpose || "";
    $("person-soul").value = person?.soul || "";
    $("person-kind").value = person?.kind || "fixed";
    $("person-kind").disabled = Boolean(person);
    const roles = $("person-role");
    roles.replaceChildren();
    const options = new Set([
        ...Object.keys(workspaceState.peopleModels),
        ...(workspaceState.peopleModels.research ? ["synthesis"] : []),
    ]);
    if (person)
        options.add(person.role);
    for (const role of options) {
        const option = el("option", workspaceState.personRoles[role] || role);
        option.value = role;
        roles.append(option);
    }
    roles.value = person?.role || "research";
    roles.disabled = Boolean(person);
    $("person-form-status").textContent = "";
    $("person-name").focus();
}
export async function openPerson(id) {
    let person = workspaceState.peopleIndex.get(id);
    if (!person) {
        await loadPeople();
        person = workspaceState.peopleIndex.get(id);
    }
    if (!person)
        return;
    workspaceState.personFocus = id;
    workspaceState.personRequest++;
    workspaceState.personHistory = [];
    workspaceState.personHistoryCursor = null;
    workspaceState.memoryNoteRequest = null;
    if ($("session-window").open)
        closeSession();
    $("person-title").textContent = person.name;
    $("person-subtitle").textContent =
        `${workspaceState.personKinds[person.kind]} · ${workspaceState.personRoles[person.role] || person.role} · ${{ offline: "离线", online: "在线", busy: "忙碌", chatting: "对话中" }[personPresence(id)]}`;
    $("person-subtitle").textContent +=
        ` · 执行偏好：${person.execution?.backend || "按任务选择"} / ${person.execution?.model || "按任务选择模型"}`;
    $("person-avatar").replaceChildren(avatar(personRepresentative(person)));
    $("person-soul-view").textContent =
        person.soul || "尚未设置独立 Soul；历史任务的角色准则保留在原会话中。";
    $("person-purpose-view").textContent = person.purpose;
    $("promote-person").hidden = person.kind === "fixed";
    $("person-status").textContent = "";
    $("person-memory-note").value = "";
    $("person-memory-query").value = "";
    $("person-memory-events").replaceChildren();
    $("person-memory-stats").textContent = "记忆由 relay-memory 管理";
    switchPersonTab("history");
    const panel = $("person-window"), embedded = !$("settings-page").hidden &&
        workspaceState.settingsCategory === "people";
    if (panel.open && panel.classList.contains("embedded") !== embedded)
        panel.close();
    panel.classList.toggle("embedded", embedded);
    $("maximize-person").hidden = embedded;
    $("close-person").textContent = embedded ? "← 返回名册" : "×";
    $("close-person").setAttribute("aria-label", embedded ? "返回数字人名册" : "关闭数字人窗口");
    if (embedded) {
        settingsPeopleView("detail");
        if (panel.parentElement !== $("person-inline-host"))
            $("person-inline-host").append(panel);
        if (!panel.open)
            panel.show();
        $("close-person").focus({ preventScroll: true });
    }
    else {
        if (panel.parentElement !== document.body)
            document.body.append(panel);
        if (!panel.open)
            panel.showModal();
    }
    await loadPersonHistory();
}
export async function loadPersonHistory(after = "") {
    const id = workspaceState.personFocus, request = workspaceState.personRequest;
    $("person-status").textContent = "正在读取会话历史…";
    $("person-history-more").disabled = true;
    try {
        const response = await fetch(`/api/people/${encodeURIComponent(id)}${after ? `?after=${encodeURIComponent(after)}` : ""}`, { signal: AbortSignal.timeout(10000) });
        if (!response.ok)
            throw new Error("会话历史暂不可读。");
        const value = await response.json();
        if (workspaceState.personFocus !== id ||
            workspaceState.personRequest !== request)
            return;
        workspaceState.peopleIndex.set(id, value.person);
        workspaceState.personHistory.push(...value.sessions);
        workspaceState.personHistoryCursor = value.next_after;
        const root = $("person-history-list");
        root.replaceChildren();
        for (const job of workspaceState.personHistory) {
            const card = el("button", undefined, "person-session"), text = el("span", job.topics?.join(" · ") || job.repository);
            text.append(el("small", `${job.backend} · ${job.model} · 第 ${job.attempt} 次执行 · ${job.session_id || job.id}`));
            card.append(text, badge(job.state));
            card.addEventListener("click", () => {
                if (!$("person-window").classList.contains("embedded"))
                    $("person-window").close();
                openSessionRecord(job);
            });
            root.append(card);
        }
        if (!workspaceState.personHistory.length)
            root.append(el("p", "还没有任务会话。将它设为默认成员，或在新任务中选用这个数字人。", "empty"));
        $("person-status").textContent =
            `已加载 ${workspaceState.personHistory.length} 轮会话`;
        $("person-history-more").hidden = !workspaceState.personHistoryCursor;
    }
    catch (error) {
        if (workspaceState.personFocus === id &&
            workspaceState.personRequest === request)
            $("person-status").textContent = error.message;
    }
    finally {
        $("person-history-more").disabled = false;
    }
}
export function switchPersonTab(tab) {
    for (const key of ["history", "memory", "execution"]) {
        $("person-" + key + "-panel").hidden = key !== tab;
        $("person-tab-" + key).classList.toggle("active", key === tab);
    }
}
export async function recallPerson() {
    const id = workspaceState.personFocus, request = workspaceState.personRequest;
    $("person-status").textContent = "正在召回记忆…";
    try {
        const result = await peopleApi({
            operation: "recall",
            id,
            query: $("person-memory-query").value,
        });
        if (id !== workspaceState.personFocus ||
            request !== workspaceState.personRequest)
            return;
        $("person-memory-stats").textContent =
            `${result.stats.event_count} 条记忆 · ${result.stats.session_count} 个会话 · ${result.stats.topic_count} 个主题`;
        const root = $("person-memory-events");
        root.replaceChildren();
        for (const event of result.events) {
            const card = el("article", undefined, "memory-event");
            card.append((() => {
                const label = el("small");
                label.append(messageTimestamp(Number(event.at)), el("span", ` · ${event.session === "profile-notes" ? "手动记录" : "研究记录"}${event.excerpt ? " · 召回片段" : ""}`));
                return label;
            })(), readable(event.response || event.summary || "", `memory:${event.id}`));
            if (event.metadata?.receipt_sha256) {
                const evidence = fold(`memory-evidence:${event.id}`, "证据来源", "protocol-detail");
                evidence.append(el("pre", JSON.stringify(event.metadata, null, 2)));
                card.append(evidence);
            }
            root.append(card);
        }
        if (!result.events.length)
            root.append(el("p", "还没有相关记忆。可以先记录一条研究线索。", "empty"));
        $("person-status").textContent = result.unconfirmed_writebacks
            ? `${result.unconfirmed_writebacks} 次写回尚未确认，请在系统消息中检查。`
            : "记忆已读取";
    }
    catch (error) {
        if (id === workspaceState.personFocus &&
            request === workspaceState.personRequest)
            $("person-status").textContent = error.message;
    }
}

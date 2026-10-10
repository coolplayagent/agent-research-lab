"use strict";
const personKinds = {
  fixed: "固定成员",
  temporary: "临时成员",
  research: "研究成员",
};
const personRoles = {
  research: "研究员",
  review: "质疑者",
  synthesis: "综合者",
  implement: "实现者",
  evaluate: "评估者",
  evaluator: "评估者",
};
let peopleRevision = -1;
let peopleIndex = new Map(),
  peopleDefaults = {},
  peopleModels = {},
  peopleTotal = 0,
  peopleCursor = null;
let personFocus = null,
  personHistory = [],
  personHistoryCursor = null,
  personRequest = 0;
let editingPerson = null,
  directoryLoading = false,
  peopleKey = "",
  memoryNoteRequest = null;
function digitalPersonId(j) {
  return j?.profile?.person_id || j?.id;
}
function currentPersonJob(id) {
  const list = (data?.jobs || []).filter((j) => digitalPersonId(j) === id);
  return (
    list.find((j) => j.state === "running") ||
    list.find((j) => j.cohort_id === room) ||
    list[0]
  );
}
function personPresence(id) {
  const list = (data?.jobs || []).filter((j) => digitalPersonId(j) === id);
  const priority = { offline: 0, online: 1, chatting: 2, busy: 3 };
  return (
    list
      .map(agentPresence)
      .sort((a, b) => priority[b.state] - priority[a.state])[0]?.state ||
    "offline"
  );
}
function personRepresentative(person) {
  return (
    currentPersonJob(person.id) || {
      id: person.id,
      role: person.role,
      model:
        peopleModels[person.role === "synthesis" ? "research" : person.role],
      state: "offline",
      profile: {
        person_id: person.id,
        display_name: person.name,
        handle: person.id,
      },
      dependencies: [],
    }
  );
}
function syncPeople(roster) {
  if (!roster) return;
  peopleTotal = roster.total || peopleTotal;
  if ((roster.revision || 0) >= peopleRevision) {
    peopleDefaults = roster.defaults || {};
    peopleModels = roster.models || {};
    peopleRevision = roster.revision || 0;
  }
  for (const p of roster.people || []) {
    if (!peopleIndex.has(p.id) || peopleIndex.get(p.id).revision <= p.revision)
      peopleIndex.set(p.id, p);
  }
  renderPeopleDirectory();
  $("people-more").hidden = !peopleCursor || peopleIndex.size >= peopleTotal;
}
async function peopleApi(body) {
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
  if (!response.ok) throw new Error(value.error || "请求未完成，请稍后重试。");
  return value;
}
async function loadPeople(after = "") {
  if (directoryLoading) return;
  directoryLoading = true;
  $("people-more").disabled = true;
  try {
    const response = await fetch(
      `/api/people${after ? `?after=${encodeURIComponent(after)}` : ""}`,
      { signal: AbortSignal.timeout(8000) },
    );
    if (!response.ok) throw new Error("名册暂不可读。");
    const value = await response.json();
    peopleCursor = value.next_after;
    syncPeople(value);
    $("people-settings-status").textContent =
      `共 ${value.total} 位数字人，已加载 ${peopleIndex.size} 位。`;
  } catch (error) {
    $("people-settings-status").textContent = error.message;
  } finally {
    directoryLoading = false;
    $("people-more").disabled = false;
    $("people-more").hidden = !peopleCursor || peopleIndex.size >= peopleTotal;
  }
}
function renderPeopleDirectory() {
  const key = JSON.stringify([
    Array.from(peopleIndex.values()),
    peopleDefaults,
    peopleModels,
    $("people-search").value,
    $("people-temporary").checked,
  ]);
  if (key === peopleKey) return;
  peopleKey = key;
  const defaults = $("people-defaults");
  defaults.replaceChildren();
  for (const [role, id] of Object.entries(peopleDefaults)) {
    const label = el("label", `${personRoles[role] || role} · 默认成员`),
      select = el("select");
    select.setAttribute("aria-label", `${personRoles[role] || role}默认成员`);
    for (const p of peopleIndex.values())
      if (p.role === role && p.kind === "fixed") {
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
        peopleDefaults[role] = select.value;
        $("people-settings-status").textContent =
          "默认成员已保存，新的研究任务会复用它。";
        await loadPeople();
        await refresh();
      } catch (error) {
        select.value = id;
        $("people-settings-status").textContent = error.message;
      } finally {
        select.disabled = false;
      }
    });
    label.append(select);
    defaults.append(label);
  }
  const root = $("people-directory"),
    query = $("people-search").value.toLowerCase();
  root.replaceChildren();
  const list = Array.from(peopleIndex.values())
    .filter(
      (p) =>
        ($("people-temporary").checked || p.kind !== "temporary") &&
        [p.name, p.role, personRoles[p.role], p.purpose]
          .join(" ")
          .toLowerCase()
          .includes(query),
    )
    .sort(
      (a, b) => b.created_at - a.created_at || a.name.localeCompare(b.name),
    );
  for (const p of list) {
    const card = el("button", undefined, "person-card"),
      text = el("span", p.name, "task-name");
    text.append(
      el(
        "small",
        `${personKinds[p.kind]} · ${personRoles[p.role] || p.role} · ${p.task_count || 0} 个任务`,
      ),
    );
    card.append(avatar(personRepresentative(p)), text);
    card.addEventListener("click", () => openPerson(p.id));
    root.append(card);
  }
  if (!list.length)
    root.append(
      el("p", "没有匹配的数字人。可以新增成员，或显示临时成员。", "empty"),
    );
}
function settingsPeopleView(view) {
  $("people-list").hidden = view !== "list";
  $("person-form").hidden = view !== "form";
  $("person-inline-host").hidden = view !== "detail";
}
function closePerson() {
  const panel = $("person-window");
  panel.close();
  personRequest++;
  if (panel.classList.contains("embedded")) {
    settingsPeopleView("list");
    panel.classList.remove("embedded");
    document.body.append(panel);
  }
}
function editPerson(person = null) {
  closePerson();
  location.hash = "settings/people";
  showPage("settings/people", true);
  editingPerson = person;
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
    ...Object.keys(peopleModels),
    ...(peopleModels.research ? ["synthesis"] : []),
  ]);
  if (person) options.add(person.role);
  for (const role of options) {
    const option = el(
      "option",
      `${personRoles[role] || role} · ${peopleModels[role === "synthesis" ? "research" : role] || "由任务绑定"}`,
    );
    option.value = role;
    roles.append(option);
  }
  roles.value = person?.role || "research";
  roles.disabled = Boolean(person);
  $("person-form-status").textContent = "";
  $("person-name").focus();
}
$("person-form").addEventListener("submit", async (event) => {
  event.preventDefault();
  $("save-person").disabled = true;
  const fields = {
    name: $("person-name").value,
    soul: $("person-soul").value,
    purpose: $("person-purpose").value,
  };
  const change = editingPerson
    ? {
        action: "update",
        id: editingPerson.id,
        revision: editingPerson.revision,
        ...fields,
      }
    : {
        action: "create",
        role: $("person-role").value,
        kind: $("person-kind").value,
        ...fields,
      };
  try {
    const result = await peopleApi({ operation: "profile", change });
    peopleIndex.set(result.person.id, result.person);
    $("person-form").hidden = true;
    renderPeopleDirectory();
    await loadPeople();
    await refresh();
    await openPerson(result.person.id);
  } catch (error) {
    $("person-form-status").textContent = error.message;
  } finally {
    $("save-person").disabled = false;
  }
});
async function openPerson(id) {
  let person = peopleIndex.get(id);
  if (!person) {
    await loadPeople();
    person = peopleIndex.get(id);
  }
  if (!person) return;
  personFocus = id;
  personRequest++;
  personHistory = [];
  personHistoryCursor = null;
  memoryNoteRequest = null;
  if ($("session-window").open) closeSession();
  $("person-title").textContent = person.name;
  $("person-subtitle").textContent =
    `${personKinds[person.kind]} · ${personRoles[person.role] || person.role} · ${{ offline: "离线", online: "在线", busy: "忙碌", chatting: "对话中" }[personPresence(id)]}`;
  $("person-subtitle").textContent +=
    ` · ${personRepresentative(person).model || "尚未绑定模型"}`;
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
  const panel = $("person-window"),
    embedded = !$("settings-page").hidden && settingsCategory === "people";
  if (panel.open && panel.classList.contains("embedded") !== embedded)
    panel.close();
  panel.classList.toggle("embedded", embedded);
  $("maximize-person").hidden = embedded;
  $("close-person").textContent = embedded ? "← 返回名册" : "×";
  $("close-person").setAttribute(
    "aria-label",
    embedded ? "返回数字人名册" : "关闭数字人窗口",
  );
  if (embedded) {
    settingsPeopleView("detail");
    if (panel.parentElement !== $("person-inline-host"))
      $("person-inline-host").append(panel);
    if (!panel.open) panel.show();
    $("close-person").focus({ preventScroll: true });
  } else {
    if (panel.parentElement !== document.body) document.body.append(panel);
    if (!panel.open) panel.showModal();
  }
  await loadPersonHistory();
}
async function loadPersonHistory(after = "") {
  const id = personFocus,
    request = personRequest;
  $("person-status").textContent = "正在读取会话历史…";
  $("person-history-more").disabled = true;
  try {
    const response = await fetch(
      `/api/people/${encodeURIComponent(id)}${after ? `?after=${encodeURIComponent(after)}` : ""}`,
      { signal: AbortSignal.timeout(10000) },
    );
    if (!response.ok) throw new Error("会话历史暂不可读。");
    const value = await response.json();
    if (personFocus !== id || personRequest !== request) return;
    peopleIndex.set(id, value.person);
    personHistory.push(...value.sessions);
    personHistoryCursor = value.next_after;
    const root = $("person-history-list");
    root.replaceChildren();
    for (const job of personHistory) {
      const card = el("button", undefined, "person-session"),
        text = el("span", job.topics?.join(" · ") || job.repository);
      text.append(
        el("small", `${job.model} · 第 ${job.attempt} 轮 · ${job.id}`),
      );
      card.append(text, badge(job.state));
      card.addEventListener("click", () => {
        if (!$("person-window").classList.contains("embedded"))
          $("person-window").close();
        openSessionRecord(job);
      });
      root.append(card);
    }
    if (!personHistory.length)
      root.append(
        el(
          "p",
          "还没有任务会话。将它设为默认成员，或在新任务中选用这个数字人。",
          "empty",
        ),
      );
    $("person-status").textContent = `已加载 ${personHistory.length} 轮会话`;
    $("person-history-more").hidden = !personHistoryCursor;
  } catch (error) {
    if (personFocus === id && personRequest === request)
      $("person-status").textContent = error.message;
  } finally {
    $("person-history-more").disabled = false;
  }
}
function switchPersonTab(tab) {
  for (const key of ["history", "memory"]) {
    $("person-" + key + "-panel").hidden = key !== tab;
    $("person-tab-" + key).classList.toggle("active", key === tab);
  }
}
async function recallPerson() {
  const id = personFocus,
    request = personRequest;
  $("person-status").textContent = "正在召回记忆…";
  try {
    const result = await peopleApi({
      operation: "recall",
      id,
      query: $("person-memory-query").value,
    });
    if (id !== personFocus || request !== personRequest) return;
    $("person-memory-stats").textContent =
      `${result.stats.event_count} 条记忆 · ${result.stats.session_count} 个会话 · ${result.stats.topic_count} 个主题`;
    const root = $("person-memory-events");
    root.replaceChildren();
    for (const event of result.events) {
      const card = el("article", undefined, "memory-event");
      card.append(
        (() => {
          const label = el("small");
          label.append(
            messageTimestamp(Number(event.at)),
            el(
              "span",
              ` · ${event.session === "profile-notes" ? "手动记录" : "研究记录"}${event.excerpt ? " · 召回片段" : ""}`,
            ),
          );
          return label;
        })(),
        readable(event.response || event.summary || "", `memory:${event.id}`),
      );
      if (event.metadata?.receipt_sha256) {
        const evidence = fold(
          `memory-evidence:${event.id}`,
          "证据来源",
          "protocol-detail",
        );
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
  } catch (error) {
    if (id === personFocus && request === personRequest)
      $("person-status").textContent = error.message;
  }
}
$("person-note-form").addEventListener("submit", async (event) => {
  event.preventDefault();
  const note = $("person-memory-note").value.trim();
  if (!note) return;
  const id = personFocus;
  memoryNoteRequest =
    memoryNoteRequest?.note === note && memoryNoteRequest.id === id
      ? memoryNoteRequest
      : { id, note, key: crypto.randomUUID() };
  $("person-remember").disabled = true;
  try {
    await peopleApi({
      operation: "remember",
      id,
      request_id: memoryNoteRequest.key,
      note,
    });
    if (personFocus === id) {
      $("person-memory-note").value = "";
      memoryNoteRequest = null;
      await recallPerson();
    }
  } catch (error) {
    if (personFocus === id) $("person-status").textContent = error.message;
  } finally {
    $("person-remember").disabled = false;
  }
});
$("person-recall-form").addEventListener("submit", (event) => {
  event.preventDefault();
  recallPerson();
});
$("person-tab-history").addEventListener("click", () =>
  switchPersonTab("history"),
);
$("person-tab-memory").addEventListener("click", () => {
  switchPersonTab("memory");
  recallPerson();
});
$("promote-person").addEventListener("click", async () => {
  const p = peopleIndex.get(personFocus);
  $("promote-person").disabled = true;
  try {
    const result = await peopleApi({
      operation: "profile",
      change: { action: "promote", id: p.id, revision: p.revision },
    });
    peopleIndex.set(p.id, result.person);
    await loadPeople();
    await refresh();
    await openPerson(p.id);
  } catch (error) {
    $("person-status").textContent = error.message;
  } finally {
    $("promote-person").disabled = false;
  }
});
$("edit-person").addEventListener("click", () =>
  editPerson(peopleIndex.get(personFocus)),
);
$("close-person").addEventListener("click", () => {
  const id = personFocus;
  closePerson();
  if (!$("settings-page").hidden) {
    const index = Array.from($("people-directory").children).find((card) =>
      card.textContent.includes(peopleIndex.get(id)?.name),
    );
    (index || $("create-person")).focus({ preventScroll: true });
  }
});
$("person-window").addEventListener("cancel", () => personRequest++);
$("person-history-more").addEventListener("click", () =>
  loadPersonHistory(personHistoryCursor),
);
$("create-person").addEventListener("click", () => editPerson());
$("cancel-person").addEventListener("click", () => {
  if (editingPerson) openPerson(editingPerson.id);
  else {
    settingsPeopleView("list");
    $("create-person").focus();
  }
});
$("people-search").addEventListener("input", renderPeopleDirectory);
$("people-temporary").addEventListener("change", renderPeopleDirectory);
$("people-more").addEventListener("click", () => loadPeople(peopleCursor));
$("nav-settings").addEventListener("click", () => loadPeople());
document.addEventListener("keydown", (event) => {
  if (
    event.key === "Escape" &&
    $("person-window").open &&
    $("person-window").classList.contains("embedded") &&
    !$("session-window").open
  ) {
    event.preventDefault();
    $("close-person").click();
  }
});
function renderPeopleSidebar(list) {
  if (!data?.roster) return;
  const root = $("tasks"),
    scroll = root.scrollTop,
    query = $("search").value.toLowerCase().replace(/^@/, "");
  const current = new Set(list.map(digitalPersonId));
  const filtered = Array.from(peopleIndex.values()).filter(
    (p) =>
      (p.kind !== "temporary" || current.has(p.id)) &&
      [
        p.id,
        p.name,
        p.role,
        p.purpose,
        personRoles[p.role],
        peopleModels[p.role],
      ]
        .join(" ")
        .toLowerCase()
        .includes(query),
  );
  const priority = { offline: 0, online: 1, chatting: 2, busy: 3 };
  filtered.sort(
    (a, b) =>
      priority[personPresence(b.id)] - priority[personPresence(a.id)] ||
      a.name.localeCompare(b.name),
  );
  root.replaceChildren();
  for (const p of filtered) {
    const j = personRepresentative(p),
      state = personPresence(p.id);
    if ($("state").value && j.state !== $("state").value) continue;
    const row = el(
        "button",
        undefined,
        `task-row ${digitalPersonId(data.jobs.find((j) => j.id === selected)) === p.id ? "selected" : ""}`,
      ),
      text = el("span", p.name, "task-name");
    text.append(
      el(
        "small",
        `${personRoles[p.role] || p.role} · ${{ offline: "离线", online: "在线", busy: "忙碌", chatting: "对话中" }[state]} · ${p.task_count || 0} 个任务`,
      ),
    );
    row.append(avatar(j), text, el("span", undefined, `presence ${state}`));
    row.title = `${personKinds[p.kind]} · @${p.name}`;
    row.addEventListener("click", () => openPerson(p.id));
    root.append(row);
  }
  root.scrollTop = scroll;
  $("task-count").textContent = root.children.length;
}

$("maximize-person").addEventListener("click", () => {
  const expanded = $("person-window").classList.toggle("expanded");
  $("maximize-person").textContent = expanded ? "↙ 还原" : "↗ 放大";
  $("maximize-person").setAttribute("aria-pressed", String(expanded));
  $("maximize-person").setAttribute(
    "aria-label",
    `${expanded ? "还原" : "放大"}数字人窗口`,
  );
});

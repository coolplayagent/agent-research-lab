"use strict";
(() => {
  let groups = [],
    next = null,
    selectedGroup = null,
    active = false,
    live = null,
    cursor = 0,
    members = [],
    memberNext = null,
    messageKey = "",
    directoryKey = "",
    fetchingGroups = false,
    again = false;
  let browsingHistory = false;
  const messages = new Map(),
    chosen = new Set();
  const display = (id) => peopleIndex.get(id)?.name || id;
  const stateLabel = {
    offline: "离线",
    online: "在线",
    chatting: "对话中",
    busy: "忙碌",
  };
  function notice(text) {
    $("crystal-form-status").textContent = text;
  }
  function failure(text) {
    serviceAlerts.set("crystal-service", text);
    renderSystemMessages();
  }
  async function api(path, body) {
    const response = await fetch(path, {
      method: body ? "POST" : "GET",
      cache: "no-store",
      headers: body
        ? {
            "Content-Type": "application/json",
            "X-Crystal-Intent": "manage-crystal",
          }
        : {},
      body: body ? JSON.stringify(body) : undefined,
      signal: AbortSignal.timeout(10000),
    });
    const value = await response.json();
    if (!response.ok) throw Error(value.error || "通信请求失败");
    return value;
  }
  async function change(body) {
    return api("/api/crystal/manage", body);
  }
  function metrics(value) {
    const root = $("crystal-metrics");
    root.replaceChildren();
    for (const [label, valueText] of [
      ["持久消息", value.stored_messages],
      ["协作群", value.groups],
      ["数字人", value.actors],
      ["实时连接", value.subscriptions],
      ["待写入消息", value.queue_depth],
      ["拒绝 / 背压", value.rejected],
      ["重放消息", value.replayed],
      ["写入确认 P99", `${value.durable_ack_ms?.p99?.toFixed(2) ?? "—"} ms`],
      ["存储状态", value.healthy ? "可用" : "需要恢复"],
    ])
      root.append(el("dt", label), el("dd", valueText ?? "—"));
  }
  async function loadGroups(after = "") {
    if (fetchingGroups) {
      again = true;
      return;
    }
    fetchingGroups = true;
    try {
      const value = await api(
        `/api/crystal/view?after=${encodeURIComponent(after)}&query=${encodeURIComponent($("crystal-search").value)}`,
      );
      groups = value.directory.groups;
      next = value.directory.next_after;
      $("crystal-next").hidden = !next;
      metrics(value.metrics);
      renderDirectory();
      render();
    } catch (e) {
      failure(e.message);
    } finally {
      fetchingGroups = false;
      if (again) {
        again = false;
        loadGroups(after);
      }
    }
  }
  function renderDirectory() {
    const root = $("crystal-directory");
    root.replaceChildren();
    for (const group of [...groups].sort(
      (a, b) => Number(b.pinned) - Number(a.pinned),
    )) {
      const button = el(
        "button",
        `${group.pinned ? "★ " : ""}${group.private ? "私聊 · " : "# "}${group.title} · ${group.member_count} 人${group.archived ? " · 已归档" : ""}`,
        "crystal-group",
      );
      button.dataset.groupId = group.id;
      button.onclick = () => select(group, false);
      root.append(button);
    }
  }
  function picker() {
    const root = $("crystal-member-picker");
    root.replaceChildren();
    const query = $("crystal-member-search").value.toLowerCase();
    for (const person of [...peopleIndex.values()]
      .filter((p) => p.name.toLowerCase().includes(query))
      .slice(0, 100)) {
      const label = el("label"),
        box = el("input");
      box.type = "checkbox";
      box.checked = chosen.has(person.id);
      box.onchange = () =>
        box.checked ? chosen.add(person.id) : chosen.delete(person.id);
      label.append(box, el("span", person.name));
      root.append(label);
    }
    $("crystal-more-people").hidden = !peopleCursor;
  }
  async function select(group, open) {
    document
      .querySelectorAll(".crystal-grant")
      .forEach((panel) => panel.remove());
    selectedGroup = group;
    members = [];
    memberNext = null;
    active = open;
    messages.clear();
    browsingHistory = false;
    older.disabled = false;
    cursor = 0;
    messageKey = "";
    setFollowing(false);
    await loadMembers();
    subscribe();
    if (open) {
      location.hash = "collaboration";
      showPage("collaboration");
    }
    render();
  }
  async function loadMembers(after = "") {
    if (!selectedGroup) return;
    const id = selectedGroup.id;
    try {
      const value = await api(
        `/api/crystal/view?group_id=${encodeURIComponent(id)}&after=${encodeURIComponent(after)}&sequence=${cursor}`,
      );
      if (selectedGroup?.id !== id) return;
      selectedGroup = value.group;
      members = after
        ? [...members, ...value.members.members].slice(-200)
        : value.members.members;
      memberNext = value.members.next_after;
      for (const m of browsingHistory ? [] : value.messages) {
        messages.set(m.sequence, m);
        cursor = Math.max(cursor, m.sequence);
      }
      renderMembers();
      if (data) renderPeopleSidebar(jobs());
      render();
    } catch (e) {
      failure(e.message);
    }
  }
  function renderMembers() {
    const root = $("crystal-members");
    root.replaceChildren();
    $("crystal-room-settings").hidden = !selectedGroup;
    if (!selectedGroup) return;
    $("crystal-selected-title").textContent = selectedGroup.title;
    $("crystal-pin").textContent = selectedGroup.pinned ? "取消置顶" : "置顶";
    $("crystal-archive").textContent = selectedGroup.archived
      ? "恢复讨论"
      : "归档";
    $("crystal-more-members").hidden = !memberNext;
    $("crystal-add-members").hidden = selectedGroup.private;
    for (const member of members) {
      const row = el("div", undefined, "crystal-member-row"),
        person = el("button", display(member.person_id));
      person.onclick = () => openPerson(member.person_id);
      const status = el("select");
      status.setAttribute("aria-label", `${display(member.person_id)} 的状态`);
      for (const [key, label] of Object.entries(stateLabel)) {
        const option = el("option", label);
        option.value = key;
        option.selected = key === member.presence;
        status.append(option);
      }
      status.onchange = async () => {
        try {
          await change({
            operation: "presence",
            person_id: member.person_id,
            state: status.value,
          });
          await loadMembers();
        } catch (e) {
          failure(e.message);
        }
      };
      row.append(person, status, el("small", `${member.connections} 个连接`));
      const credential = el("button", "连接凭据"),
        remove = el("button", "移出会话");
      credential.onclick = async () => {
        try {
          const value = await change({
            operation: "grant",
            person_id: member.person_id,
            lifetime_seconds: 3600,
          });
          const panel = el("div", undefined, "crystal-grant"),
            secret = el("input"),
            copy = el("button", "复制"),
            dismiss = el("button", "关闭");
          secret.type = "password";
          secret.readOnly = true;
          secret.value = value.token;
          secret.setAttribute("aria-label", "仅显示一次的会话凭据");
          copy.onclick = async () => {
            await navigator.clipboard.writeText(secret.value);
            copy.textContent = "已复制";
          };
          dismiss.onclick = () => {
            secret.value = "";
            panel.remove();
          };
          panel.append(
            el(
              "p",
              "此凭据仅用于该数字人的宿主适配器，一小时后失效。先前凭据已撤销。",
            ),
            secret,
            copy,
            dismiss,
          );
          $("crystal-room-settings").append(panel);
        } catch (e) {
          failure(e.message);
        }
      };
      remove.onclick = async () => {
        try {
          await change({
            operation: "membership",
            group_id: selectedGroup.id,
            person_id: member.person_id,
            add: false,
          });
          await loadMembers();
        } catch (e) {
          failure(e.message);
        }
      };
      row.append(credential, remove);
      root.append(row);
    }
  }
  function subscribe() {
    live?.close();
    const id = selectedGroup?.id || "";
    live = new EventSource(
      `/api/crystal/events?group_id=${encodeURIComponent(id)}&sequence=${cursor}`,
    );
    live.addEventListener("status", (event) => {
      const value = JSON.parse(event.data);
      metrics(value.metrics);
      if (value.group && selectedGroup?.id === value.group.id) {
        const changed = value.group.revision !== selectedGroup.revision;
        selectedGroup = value.group;
        if (changed) loadMembers();
      }
      const key = JSON.stringify([
        value.metrics.catalog_revision,
        value.group?.revision,
      ]);
      if (key !== directoryKey) {
        directoryKey = key;
        loadGroups();
        if (selectedGroup) loadMembers();
      }
      render();
    });
    live.addEventListener("message", (event) => {
      if (selectedGroup?.id !== id) return;
      const message = JSON.parse(event.data);
      cursor = Math.max(cursor, message.sequence);
      if (browsingHistory) return;
      messages.set(message.sequence, message);
      while (messages.size > 256) messages.delete(Math.min(...messages.keys()));
      render();
    });
    live.addEventListener("fault", () =>
      failure("协作消息读取失败，等待恢复。"),
    );
    live.onerror = () => failure("协作消息连接中断，正在重连。历史消息保留。");
    live.onopen = () => {
      serviceAlerts.delete("crystal-service");
      renderSystemMessages();
    };
  }
  function render() {
    picker();
    const root = $("rooms");
    root.querySelector(".crystal-live-rooms")?.remove();
    const section = el("div", undefined, "crystal-live-rooms");
    const privateRoot = $("private-chats");
    privateRoot.querySelector(".crystal-live-private")?.remove();
    const privateSection = el("div", undefined, "crystal-live-private");
    for (const group of [...groups].sort(
      (a, b) => Number(b.pinned) - Number(a.pinned),
    )) {
      const button = el(
        "button",
        `${group.pinned ? "★ " : ""}${group.private ? "↗ " : "# "}${group.title}`,
        `room-button ${active && selectedGroup?.id === group.id ? "selected" : ""}`,
      );
      button.dataset.groupId = group.id;
      button.title = group.topic;
      button.onclick = () => select(group, true);
      (group.private ? privateSection : section).append(button);
    }
    const manage = el("button", "＋ 管理会话", "latest-button");
    manage.onclick = () => {
      location.hash = "settings/communication";
      showPage("settings/communication");
    };
    section.append(manage);
    root.prepend(section);
    privateRoot.prepend(privateSection);
    $("room-count").textContent =
      new Set((data?.jobs || []).map((j) => j.cohort_id).filter(Boolean)).size +
      groups.filter((g) => !g.private).length;
    $("private-count").textContent =
      privateRoot.querySelectorAll(".private-chat").length +
      groups.filter((g) => g.private).length;
    older.hidden = latest.hidden = !active;
    if (!active || !selectedGroup) return;
    $("room-title").textContent =
      `${selectedGroup.private ? "私聊" : "#"} ${selectedGroup.title}`;
    $("room-subtitle").textContent = selectedGroup.topic;
    $("chat-title").textContent = selectedGroup.private
      ? "私聊记录"
      : "# 主题讨论";
    $("graph-limit").textContent =
      `${selectedGroup.member_count} 名成员 · 当前显示 ${Math.min(16, members.length)} 名`;
    $("message-count").textContent = `最近 ${messages.size} 条消息`;
    const key = JSON.stringify([
      selectedGroup.id,
      [...messages.keys()].sort((a, b) => a - b),
      members.map((m) => [m.person_id, m.presence]),
    ]);
    if (
      key === messageKey &&
      $("timeline").dataset.crystal === selectedGroup.id
    )
      return;
    messageKey = key;
    const graph = $("graph");
    graph.replaceChildren();
    const shown = members.slice(0, 16),
      height = Math.max(320, Math.ceil(shown.length / 2) * 64 + 60);
    const canvas = svg("svg", {
      viewBox: `0 0 900 ${height}`,
      role: "img",
      "aria-label": "数字人与水晶球公共板的消息关系",
    });
    const cy = height / 2,
      last = [...messages.values()]
        .sort((a, b) => a.sequence - b.sequence)
        .at(-1);
    const positions = new Map(
      shown.map((m, i) => [
        m.person_id,
        { x: i % 2 ? 730 : 40, y: 35 + Math.floor(i / 2) * 64 },
      ]),
    );
    for (const member of shown) {
      const pos = positions.get(member.person_id),
        sending = last?.sender_id === member.person_id;
      canvas.append(
        svg("path", {
          d: `M ${pos.x + 65} ${pos.y + 25} Q 450 ${pos.y + 25} 450 ${cy}`,
          class: sending ? "publish traffic" : "membership",
        }),
      );
    }
    const board = svg("g", { class: "board" });
    board.append(
      svg("circle", { cx: 450, cy, r: 70, fill: "#ecfbff", stroke: "#7abfcb" }),
      svg(
        "text",
        { x: 450, y: cy - 8, "text-anchor": "middle" },
        selectedGroup.title.slice(0, 18),
      ),
      svg(
        "text",
        { x: 450, y: cy + 16, "text-anchor": "middle", class: "board-sub" },
        `${selectedGroup.member_count} 名成员 · 公共板`,
      ),
    );
    canvas.append(board);
    for (const member of shown) {
      const pos = positions.get(member.person_id),
        node = svg("g", {
          transform: `translate(${pos.x},${pos.y})`,
          class: "node",
          role: "button",
          tabindex: "0",
          "aria-label": `打开 ${display(member.person_id)} 的数字人看板`,
        });
      node.append(
        svg("rect", { width: 130, height: 48, rx: 10 }),
        svg("circle", {
          cx: 13,
          cy: 15,
          r: 3,
          fill: {
            offline: "#a7b3b7",
            online: "#199377",
            chatting: "#159aa4",
            busy: "#c68a3e",
          }[member.presence],
        }),
        svg("text", { x: 24, y: 20 }, display(member.person_id).slice(0, 10)),
        svg(
          "text",
          { x: 12, y: 37, class: "node-sub" },
          stateLabel[member.presence],
        ),
      );
      node.onclick = () => openPerson(member.person_id);
      node.onkeydown = (event) => {
        if (event.key === "Enter" || event.key === " ") {
          event.preventDefault();
          openPerson(member.person_id);
        }
      };
      canvas.append(node);
    }
    graph.append(canvas);
    const timeline = $("timeline"),
      bottom =
        timeline.scrollHeight - timeline.clientHeight - timeline.scrollTop < 80,
      scroll = timeline.scrollTop;
    timeline.replaceChildren();
    timeline.dataset.crystal = selectedGroup.id;
    for (const message of [...messages.values()].sort(
      (a, b) => a.sequence - b.sequence,
    )) {
      const row = el("article", undefined, "chat-message");
      row.dataset.sequence = message.sequence;
      const face = el(
        "button",
        display(message.sender_id).slice(0, 2),
        "avatar-button crystal-face",
      );
      face.onclick = () => openPerson(message.sender_id);
      const body = el("div", undefined, "chat-message-body"),
        header = el("div", undefined, "message-header"),
        sender = el("button", display(message.sender_id), "sender");
      sender.onclick = face.onclick;
      header.append(sender, messageTimestamp(message.accepted_ms / 1000));
      const bubble = el("div", undefined, "chat-bubble");
      if (message.reply_to) {
        const parent = messages.get(message.reply_to);
        bubble.append(
          el(
            "span",
            parent ? `@${display(parent.sender_id)}` : "回复较早的消息",
            "mention",
          ),
        );
      }
      bubble.append(el("p", message.text));
      body.append(header, bubble);
      row.append(face, body);
      timeline.append(row);
    }
    if (!messages.size)
      timeline.append(el("div", "从共同的疑问开始，倾听并回应同伴。", "empty"));
    timeline.scrollTop = bottom ? timeline.scrollHeight : scroll;
  }
  $("crystal-create").onsubmit = async (event) => {
    event.preventDefault();
    try {
      if (!chosen.size) throw Error("请选择参与讨论的数字人。");
      const group = await change({
        operation: "create",
        group: {
          id: `room-${crypto.randomUUID()}`,
          title: $("crystal-title").value,
          topic: $("crystal-topic").value,
          private: $("crystal-private").value === "true",
          members: [...chosen],
        },
      });
      notice("会话已创建。");
      await loadGroups();
      await select(group, false);
    } catch (e) {
      notice(e.message);
    }
  };
  for (const [id, field] of [
    ["crystal-pin", "pinned"],
    ["crystal-archive", "archived"],
  ])
    $(id).onclick = async () => {
      if (!selectedGroup) return;
      try {
        const { id, revision, title, topic, archived, pinned } = selectedGroup;
        await change({
          operation: "update",
          change: {
            id,
            revision,
            title,
            topic,
            archived,
            pinned,
            [field]: !selectedGroup[field],
          },
        });
        await loadMembers();
        await loadGroups();
      } catch (e) {
        failure(e.message);
      }
    };
  $("crystal-open").onclick = () => {
    active = true;
    location.hash = "collaboration";
    showPage("collaboration");
    render();
  };
  $("crystal-search-button").onclick = () => loadGroups();
  $("crystal-next").onclick = () => loadGroups(next || "");
  $("crystal-add-members").onclick = async () => {
    if (!selectedGroup || selectedGroup.private) return;
    try {
      for (const person_id of chosen)
        await change({
          operation: "membership",
          group_id: selectedGroup.id,
          person_id,
          add: true,
        });
      await loadMembers();
    } catch (e) {
      failure(e.message);
    }
  };
  $("crystal-more-members").onclick = () => loadMembers(memberNext || "");
  $("crystal-member-search").oninput = picker;
  $("crystal-more-people").onclick = async () => {
    await loadPeople(peopleCursor || "");
    picker();
  };
  const older = el("button", "↑ 较早", "history-button"),
    latest = el("button", "最新", "history-button");
  older.hidden = latest.hidden = true;
  older.setAttribute("aria-label", "查看较早的群消息");
  document.querySelector(".chat-heading").append(older, latest);
  older.onclick = async () => {
    if (!selectedGroup || !messages.size) return;
    try {
      const first = Math.min(...messages.keys());
      const value = await api(
        `/api/crystal/view?group_id=${encodeURIComponent(selectedGroup.id)}&before=${first}`,
      );
      if (!value.messages.length) {
        older.disabled = true;
        return;
      }
      browsingHistory = true;
      messages.clear();
      for (const message of value.messages)
        messages.set(message.sequence, message);
      render();
    } catch (e) {
      failure(e.message);
    }
  };
  latest.onclick = async () => {
    browsingHistory = false;
    older.disabled = false;
    cursor = 0;
    messages.clear();
    await loadMembers();
    subscribe();
    $("timeline").scrollTop = $("timeline").scrollHeight;
  };
  window.CrystalRooms = {
    render,
    isActive: () => active,
    presence: (id) =>
      members.find((member) => member.person_id === id)?.presence,
    leave() {
      active = false;
      chatKey = "";
      $("timeline").removeAttribute("data-crystal");
      messageKey = "";
      older.hidden = latest.hidden = true;
    },
  };
  window.addEventListener("hashchange", () => {
    if (location.hash !== "#settings/communication")
      document
        .querySelectorAll(".crystal-grant")
        .forEach((panel) => panel.remove());
  });
  loadGroups();
  subscribe();
})();

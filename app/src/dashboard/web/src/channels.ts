import { workspaceState } from "./state.js";
import { name, avatar, choose, mention, readable } from "./identity.js";
import { $, el, messageTimestamp, short } from "./ui.js";
import { setFollowing, savePreferences } from "./preferences.js";
import { render } from "./observer.js";
import { roomName } from "./navigation.js";
export function channelKey(record?: any): any {
  const recipients = record.message.proposal.recipients || [];
  if (!recipients.length) return "";
  return JSON.stringify([
    record.cohort_id,
    [...new Set<any>([record.message.task_id, ...recipients])].sort(),
  ]);
}
export function conversationRecords(): any {
  return workspaceState.data.boards
    .filter(
      (b?: any) => !workspaceState.room || b.cohort_id === workspaceState.room,
    )
    .flatMap((b?: any) =>
      (b.retained_messages || []).map((r?: any) => ({
        ...r,
        cohort_id: b.cohort_id,
      })),
    );
}
export function privateChats(): any {
  const byId = new Map<any, any>(
      workspaceState.data.jobs.map((j?: any) => [j.id, j]),
    ),
    channels = new Map<any, any>();
  for (const record of conversationRecords()) {
    const key = channelKey(record);
    if (!key) continue;
    if (!channels.has(key)) {
      const ids = [
        ...new Set<any>([
          record.message.task_id,
          ...record.message.proposal.recipients,
        ]),
      ].sort();
      channels.set(key, {
        label: ids
          .map((id?: any) => (byId.has(id) ? name(byId.get(id)) : id))
          .join("、"),
        count: 0,
      });
    }
    channels.get(key).count++;
  }
  if (workspaceState.chatRoute && !channels.has(workspaceState.chatRoute))
    workspaceState.chatRoute = "";
  const root = $("private-chats"),
    scroll = root.scrollTop;
  root.replaceChildren();
  $("private-count").textContent = channels.size || "";
  for (const [key, channel] of channels) {
    const button = el(
      "button",
      undefined,
      `private-chat ${workspaceState.chatRoute === key ? "selected" : ""}`,
    );
    button.append(
      el("span", "◈", "private-icon"),
      el("span", channel.label, "private-name"),
      el("small", channel.count),
    );
    button.title = channel.label;
    button.addEventListener("click", () => {
      workspaceState.chatRoute = key;
      setFollowing(false);
      render();
    });
    root.append(button);
  }
  if (!channels.size)
    root.append(el("p", "私聊与多人会话会显示在这里", "private-empty"));
  root.scrollTop = scroll;
  $("chat-title").textContent = workspaceState.chatRoute
    ? `◈ ${channels.get(workspaceState.chatRoute).label}`
    : "# 协作群聊";
  $("chat-title").title = workspaceState.chatRoute
    ? "水晶球定向分发 · 点击返回公共群聊"
    : "水晶球公共群聊";
}
export function timeline(list?: any): any {
  const root = $("timeline"),
    ids = new Set<any>(list.map((j?: any) => j.id)),
    byId = new Map<any, any>(
      workspaceState.data.jobs.map((j?: any) => [j.id, j]),
    );
  const allRecords = conversationRecords();
  const records = allRecords
    .filter(
      (m?: any) =>
        channelKey(m) === workspaceState.chatRoute &&
        ids.has(m.message.task_id),
    )
    .sort((a?: any, b?: any) => a.message.created_at - b.message.created_at);
  const sources = new Map<any, any>(
    allRecords.map((m?: any) => [m.message.id, m]),
  );
  const key = JSON.stringify([
    workspaceState.room,
    workspaceState.chatRoute,
    new Date().toDateString(),
    records,
    list.map((j?: any) => [j.id, name(j)]),
  ]);
  if (key === workspaceState.chatKey) return;
  workspaceState.chatKey = key;
  const view = `${workspaceState.room}:${workspaceState.chatRoute}`;
  const changedView = view !== workspaceState.chatView;
  workspaceState.chatView = view;
  const bottom = root.scrollHeight - root.clientHeight - root.scrollTop < 80,
    scroll = root.scrollTop;
  root.replaceChildren();
  $("message-count").textContent = `${records.length} 条消息`;
  if (!records.length)
    root.append(
      el("div", "还没有群聊消息。数字人的会话可从左侧头像打开。", "empty"),
    );
  for (const record of records.slice(-256)) {
    const m = record.message,
      p = m.proposal,
      j = byId.get(m.task_id);
    const row = el("article", undefined, "chat-message");
    row.id = "msg-" + m.id;
    const face = el("button", undefined, "avatar-button"),
      body = el("div", undefined, "chat-message-body"),
      sender = el("button", name(j), "sender");
    face.setAttribute("aria-label", `打开 ${name(j)} 的会话`);
    face.append(avatar(j));
    face.addEventListener("click", () => choose(j.id, true));
    sender.addEventListener("click", () => choose(j.id, true));
    const bubble = el("div", undefined, "chat-bubble");
    if (record.origin.state === "revoked") {
      bubble.append(el("p", "这条消息已撤销。", "withdrawn-message"));
      bubble.title = record.origin.reason || "";
    } else {
      if (p.recipients?.length) {
        for (const id of p.recipients) {
          const recipient = byId.get(id);
          bubble.append(
            recipient
              ? mention(recipient)
              : el("span", `@${id}`, "mention unavailable"),
          );
        }
      } else if (p.reply_to) {
        const parent = sources.get(p.reply_to),
          recipient = byId.get(parent?.message.task_id);
        if (recipient) bubble.append(mention(recipient));
        else
          bubble.append(
            el(
              "span",
              parent ? `@${parent.message.task_id}` : "@原消息作者",
              "mention unavailable",
            ),
          );
      }
      bubble.append(readable(p.text, `board-message:${m.id}`));
      if (record.delivery?.length) {
        const pending = record.delivery.filter(
            (d?: any) => d.state === "pending",
          ),
          expired = record.delivery.filter((d?: any) => d.state === "expired"),
          marker = el(
            "span",
            pending.length ? "◷" : expired.length ? "◌" : "✓",
            "delivery-marker",
          ),
          summary = record.delivery
            .map(
              (d?: any) =>
                `${byId.has(d.task_id) ? name(byId.get(d.task_id)) : d.task_id}：${{ pending: "待投递", delivered: "已投递", expired: "已过期" }[d.state] || d.state}`,
            )
            .join("；");
        marker.title = `${summary}。已投递不代表已读。`;
        marker.setAttribute("aria-label", marker.title);
        marker.tabIndex = 0;
        bubble.append(marker);
      }
      if (record.expired) bubble.title = "历史消息，已过期";
    }
    const header = el("div", undefined, "message-header");
    header.append(sender, messageTimestamp(m.created_at));
    body.append(header, bubble);
    row.append(face, body);
    root.append(row);
  }
  root.scrollTop = changedView || bottom ? root.scrollHeight : scroll;
}
export function rooms(): any {
  const container = $("rooms");
  container.replaceChildren();
  const groups = new Map<any, any>();
  workspaceState.data.jobs.forEach((j?: any) => {
    if (j.cohort_id && !groups.has(j.cohort_id))
      groups.set(j.cohort_id, roomName(j.cohort_id));
  });
  $("room-count").textContent = groups.size;
  const add = (id?: any, label?: any) => {
    const list = workspaceState.data.jobs.filter(
        (j?: any) => !id || j.cohort_id === id,
      ),
      button = el(
        "button",
        undefined,
        `room-button ${id === workspaceState.room ? "selected" : ""}`,
      ),
      text = el("span", label, "room-name");
    text.append(
      el(
        "small",
        `${list.filter((j?: any) => j.state === "running").length} 个活跃 · ${short(id) || "包括历史记录"}`,
      ),
    );
    button.append(
      el("span", "#", "room-icon"),
      text,
      el("span", list.length, "room-count"),
    );
    button.title = label;
    button.addEventListener("click", () => {
      workspaceState.room = id;
      workspaceState.inspected = "";
      setFollowing(false);
      workspaceState.chatRoute = "";
      workspaceState.chatKey = "";
      render();
    });
    const entry = el("div", undefined, "room-entry");
    entry.append(button);
    if (id) {
      const pinned = (workspaceState.preferences.pinnedRooms || []).includes(
          id,
        ),
        pin = el(
          "button",
          pinned ? "★" : "☆",
          `room-pin ${pinned ? "pinned" : ""}`,
        );
      pin.title = `${pinned ? "取消置顶" : "置顶"} ${label}`;
      pin.dataset.cohort = id;
      pin.setAttribute("aria-label", pin.title);
      pin.setAttribute("aria-pressed", String(pinned));
      pin.addEventListener("click", () => {
        const pins = new Set<any>(workspaceState.preferences.pinnedRooms || []);
        if (pins.has(id)) pins.delete(id);
        else pins.add(id);
        workspaceState.preferences.pinnedRooms = [...pins].slice(-1000);
        savePreferences();
        rooms();
        const buttons = container.querySelectorAll(".room-pin") as any;
        [...buttons]
          .find((b?: any) => b.dataset.cohort === id)
          ?.focus({ preventScroll: true });
      });
      entry.append(pin);
    }
    container.append(entry);
  };
  const pinned = new Set<any>(workspaceState.preferences.pinnedRooms || []);
  [...groups]
    .sort(([a]: any, [b]: any) => Number(pinned.has(b)) - Number(pinned.has(a)))
    .forEach(([id, label]: any) => add(id, label));
  add("", "全部历史任务");
  $("latest-room").classList.toggle("following", workspaceState.following);
}

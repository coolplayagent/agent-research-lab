import { OriginStateKind, DeliveryState } from "/assets/shared/contracts.js";
import { label } from "./i18n.js";
import { ExecutionState } from "/assets/shared/contracts.js";
import { t } from "./i18n.js";
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
    root.append(el("p", t("channels.507593966c"), "private-empty"));
  root.scrollTop = scroll;
  $("chat-title").textContent = workspaceState.chatRoute
    ? `◈ ${channels.get(workspaceState.chatRoute).label}`
    : t("channels.063ab25cde");
  $("chat-title").title = workspaceState.chatRoute
    ? t("channels.4ce0478a53")
    : t("channels.9015ad47ab");
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
  $("message-count").textContent = t("channels.440a478894", {
    p0: records.length,
  });
  if (!records.length)
    root.append(el("div", t("channels.978a64f29a"), "empty"));
  for (const record of records.slice(-256)) {
    const m = record.message,
      p = m.proposal,
      j = byId.get(m.task_id);
    const row = el("article", undefined, "chat-message");
    row.id = "msg-" + m.id;
    const face = el("button", undefined, "avatar-button"),
      body = el("div", undefined, "chat-message-body"),
      sender = el("button", name(j), "sender");
    face.setAttribute("aria-label", t("channels.1b73d2a745", { p0: name(j) }));
    face.append(avatar(j));
    face.addEventListener("click", () => choose(j.id, true));
    sender.addEventListener("click", () => choose(j.id, true));
    const bubble = el("div", undefined, "chat-bubble");
    if (record.origin.state === OriginStateKind.Revoked) {
      bubble.append(el("p", t("channels.b4f4cb4782"), "withdrawn-message"));
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
              parent ? `@${parent.message.task_id}` : t("channels.d718850c10"),
              "mention unavailable",
            ),
          );
      }
      bubble.append(readable(p.text, `board-message:${m.id}`));
      if (record.delivery?.length) {
        const pending = record.delivery.filter(
            (d?: any) => d.state === DeliveryState.Pending,
          ),
          expired = record.delivery.filter(
            (d?: any) => d.state === DeliveryState.Expired,
          ),
          marker = el(
            "span",
            pending.length ? "◷" : expired.length ? "◌" : "✓",
            "delivery-marker",
          ),
          summary = record.delivery
            .map(
              (d?: any) =>
                `${byId.has(d.task_id) ? name(byId.get(d.task_id)) : d.task_id}：${label("DeliveryState", d.state)}`,
            )
            .join("；");
        marker.title = t("channels.d269d55491", { p0: summary });
        marker.setAttribute("aria-label", marker.title);
        marker.tabIndex = 0;
        bubble.append(marker);
      }
      if (record.expired) bubble.title = t("channels.5719003307");
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
        t("channels.47b069aa6d", {
          p0: list.filter((j?: any) => j.state === ExecutionState.Running)
            .length,
          p1: short(id) || t("channels.2d353f0539"),
        }),
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
      pin.title = `${pinned ? t("channels.c92179b74a") : t("channels.173f88d28e")} ${label}`;
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
  add("", t("channels.24b8944237"));
  $("latest-room").classList.toggle("following", workspaceState.following);
}

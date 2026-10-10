import { GroupKind } from "/assets/shared/contracts.js";
import { t } from "./i18n.js";
import { api, change, query } from "./api.js";
import { button, el, get, notice } from "./dom.js";
import { display } from "./people.js";
import type { Group, GroupView, Message } from "./types.js";
let group: Group | null = null,
  generation = 0,
  browsing = false,
  reply: number | null = null;
const messages = new Map<number, Message>();
let pending: {
  group_id: string;
  request_id: string;
  text: string;
  reply_to: number | null;
} | null = null;
export function resetMessages(selected: Group, initial: Message[]): void {
  generation++;
  group = selected;
  browsing = false;
  reply = null;
  pending = null;
  messages.clear();
  initial.forEach((m) => messages.set(m.sequence, m));
  get<HTMLTextAreaElement>("message-text").value = "";
  get<HTMLButtonElement>("send-message").disabled = false;
  get<HTMLButtonElement>("older-messages").disabled = false;
  updateReply();
  renderMessages();
  updateComposer(selected);
}
export function updateComposer(selected: Group): void {
  group = selected;
  get<HTMLTextAreaElement>("message-text").disabled = selected.archived;
  get<HTMLButtonElement>("send-message").disabled = selected.archived;
  get("composer-label").textContent = selected.archived
    ? t("messages.39338dbfe1")
    : selected.kind === GroupKind.Board
      ? t("messages.b921d5ef68")
      : t("messages.f32eb53703");
}
export function incoming(message: Message): void {
  if (message.group_id !== group?.id || browsing) return;
  messages.set(message.sequence, message);
  while (messages.size > 256) messages.delete(Math.min(...messages.keys()));
  renderMessages();
}
function updateReply(): void {
  get("reply-status").textContent = reply
    ? t("messages.a40f9e2d59", { p0: reply })
    : "";
  get("cancel-reply").hidden = reply === null;
}
function renderMessages(): void {
  const root = get("messages"),
    bottom = root.scrollHeight - root.scrollTop - root.clientHeight < 80;
  root.replaceChildren();
  get("history-status").textContent = browsing
    ? t("messages.f1fcde6deb")
    : t("messages.2bd1c987a4");
  for (const message of [...messages.values()].sort(
    (a, b) => a.sequence - b.sequence,
  )) {
    const row = el(
      "article",
      undefined,
      "message" + (message.sender_id === "operator" ? " own" : ""),
    );
    const heading = el("header"),
      stamp = el("time", new Date(message.accepted_ms).toLocaleString("zh-CN"));
    stamp.dateTime = new Date(message.accepted_ms).toISOString();
    heading.append(
      el("strong", display(message.sender_id)),
      stamp,
      button(t("messages.cf945e21dd"), () => {
        reply = message.sequence;
        updateReply();
        get("message-text").focus();
      }),
    );
    if (message.reply_to)
      row.append(
        el("small", t("messages.a40f9e2d59", { p0: message.reply_to })),
      );
    row.append(
      heading,
      el(
        "p",
        message.event
          ? t("GoalEvent." + message.event.kind, {
              title: message.event.title,
              person: display(message.event.person_id || "operator"),
              attempt: message.event.attempt,
            })
          : message.text,
      ),
    );
    row.dataset.sequence = String(message.sequence);
    root.append(row);
  }
  if (!messages.size)
    root.append(
      el(
        "p",
        group?.kind === GroupKind.Board
          ? t("messages.7bd4c97a5a")
          : t("messages.25b7624487"),
        "empty",
      ),
    );
  if (bottom) root.scrollTop = root.scrollHeight;
}
export function initMessages(): void {
  get("cancel-reply").onclick = () => {
    reply = null;
    updateReply();
  };
  get("older-messages").onclick = async () => {
    if (!group || !messages.size) return;
    const request = generation,
      id = group.id;
    try {
      const page = await api<GroupView>(
        "/api/crystal/view?" +
          query({ group_id: id, before: Math.min(...messages.keys()) }),
      );
      if (request !== generation) return;
      if (!page.messages.length) {
        get<HTMLButtonElement>("older-messages").disabled = true;
        return;
      }
      browsing = true;
      messages.clear();
      page.messages.forEach((m) => messages.set(m.sequence, m));
      renderMessages();
      get("messages").scrollTop = 0;
    } catch (error) {
      notice(error);
    }
  };
  get("latest-messages").onclick = async () => {
    if (!group) return;
    const request = generation;
    try {
      const page = await api<GroupView>(
        "/api/crystal/view?" + query({ group_id: group.id }),
      );
      if (request !== generation) return;
      browsing = false;
      messages.clear();
      page.messages.forEach((m) => messages.set(m.sequence, m));
      get<HTMLButtonElement>("older-messages").disabled = false;
      renderMessages();
      get("messages").scrollTop = get("messages").scrollHeight;
    } catch (error) {
      notice(error);
    }
  };
  get<HTMLFormElement>("composer").onsubmit = async (event) => {
    event.preventDefault();
    if (!group || group.archived) return;
    const input = get<HTMLTextAreaElement>("message-text"),
      text = input.value.trim(),
      id = group.id,
      request = generation;
    if (!text) return;
    if (
      !pending ||
      pending.text !== text ||
      pending.reply_to !== reply ||
      pending.group_id !== id
    )
      pending = {
        group_id: id,
        request_id: crypto.randomUUID(),
        text,
        reply_to: reply,
      };
    get<HTMLButtonElement>("send-message").disabled = true;
    try {
      const result = await change<{
        message: Message;
      }>({
        operation: "send",
        message: pending,
      });
      if (request !== generation) return;
      pending = null;
      if (input.value.trim() === text) input.value = "";
      reply = null;
      updateReply();
      incoming(result.message);
      notice(t("messages.60823aaec7"));
    } catch (error) {
      notice(error);
    } finally {
      if (request === generation)
        get<HTMLButtonElement>("send-message").disabled =
          group?.archived ?? true;
    }
  };
}

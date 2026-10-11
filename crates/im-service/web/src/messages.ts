import { GroupKind } from "/assets/shared/contracts.js";
import { t } from "./i18n.js";
import { goalSnapshot, focusGoal } from "./goals.js";
import { initMentions, insertMention, prepareDiscussion } from "./mentions.js";
import { messageRow } from "./message-view.js";
import { api, change, query } from "./api.js";
import { el, get, notice } from "./dom.js";
import type { Group, GroupView, Message } from "./types.js";
let group: Group | null = null,
  generation = 0,
  browsing = false,
  reply: number | null = null;
let sendingGeneration: number | null = null;
const messages = new Map<number, Message>();
const drafts = new Map<string, { text: string; reply: number | null }>();
export function messageSnapshot(): Message[] {
  return [...messages.values()];
}
export function suspendMessages(): void {
  if (group)
    drafts.set(group.id, {
      text: get<HTMLTextAreaElement>("message-text").value,
      reply,
    });
  group = null;
  generation++;
}
function sizeComposer(): void {
  const input = get<HTMLTextAreaElement>("message-text");
  input.style.height = "46px";
  input.style.height = Math.min(130, Math.max(46, input.scrollHeight)) + "px";
}
export function locateMessage(sequence: number): void {
  for (const row of get("messages").querySelectorAll<HTMLElement>(
    "[data-sequence]",
  )) {
    row.classList.toggle("target", row.dataset.sequence === String(sequence));
    if (row.dataset.sequence === String(sequence))
      row.scrollIntoView({ block: "center" });
  }
}

let pendingGoal: Record<string, unknown> | null | undefined;
let pending: {
  group_id: string;
  request_id: string;
  text: string;
  reply_to: number | null;
} | null = null;
export function resetMessages(selected: Group, initial: Message[]): void {
  if (group) suspendMessages();
  generation++;
  group = selected;
  browsing = false;
  reply = drafts.get(selected.id)?.reply ?? null;
  pending = null;
  pendingGoal = undefined;
  sendingGeneration = null;
  messages.clear();
  initial.forEach((m) => messages.set(m.sequence, m));
  get<HTMLTextAreaElement>("message-text").value =
    drafts.get(selected.id)?.text || "";
  sizeComposer();
  get<HTMLButtonElement>("send-message").disabled = false;
  get<HTMLButtonElement>("older-messages").disabled = false;
  updateReply();
  renderMessages();
  get("messages").scrollTop = get("messages").scrollHeight;
  updateComposer(selected);
}
export function updateComposer(selected: Group): void {
  group = selected;
  const input = get<HTMLTextAreaElement>("message-text"),
    submit = get<HTMLButtonElement>("send-message"),
    sending = sendingGeneration === generation;
  input.disabled = selected.archived;
  input.readOnly = sending;
  submit.disabled = selected.archived || sending || !input.value.trim();
  submit.textContent = sending ? t("ux.sending") : t("html.edecf0ae6e");
  submit.setAttribute("aria-busy", String(sending));
  get("message-count").textContent = t("ux.messageCount", { count: input.value.length });
  get("composer-label").textContent = selected.archived
    ? t("messages.39338dbfe1")
    : selected.kind === GroupKind.Board
      ? t("messages.b921d5ef68")
      : t("messages.f32eb53703");
}
export function incoming(message: Message): void {
  if (message.group_id !== group?.id || browsing) return;
  const fresh = !messages.has(message.sequence);
  messages.set(message.sequence, message);
  while (messages.size > 256) messages.delete(Math.min(...messages.keys()));
  renderMessages();
  if (fresh) document.dispatchEvent(new CustomEvent("workspace-message", { detail: message }));
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
    root.append(messageRow(message, messages, goalSnapshot(), {
      mention: insertMention,
      locate: locateMessage,
      review: focusGoal,
      reply: () => {
        reply = message.sequence;
        const author = message.event?.person_id || message.sender_id;
        if (author !== "operator") insertMention(author);
        updateReply(); get("message-text").focus();
      },
    }));
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
  document.dispatchEvent(new Event("workspace-messages"));
}
export function initMessages(): void {
  initMentions();
  document.addEventListener("workspace-goals", renderMessages);
  get("message-text").addEventListener("input", sizeComposer);
  const input = get<HTMLTextAreaElement>("message-text");
  input.addEventListener("input", () => { if (group) updateComposer(group); });
  input.addEventListener("keydown", (event) => {
    if (event.key === "Enter" && (event.ctrlKey || event.metaKey) && !event.isComposing) {
      event.preventDefault();
      if (!get<HTMLButtonElement>("send-message").disabled) get<HTMLFormElement>("composer").requestSubmit();
    }
  });
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
    if (!group || group.archived || sendingGeneration === generation) return;
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
    ) {
      pendingGoal = undefined;
      pending = {
        group_id: id,
        request_id: crypto.randomUUID(),
        text,
        reply_to: reply,
      };
    }
    sendingGeneration = generation;
    updateComposer(group);
    try {
      if (pendingGoal === undefined) {
        const goal = await prepareDiscussion(id, text);
        if (request !== generation) return;
        pendingGoal = goal;
      }
      const result = pendingGoal
        ? await api<{ message: Message }>("/api/im/discussion", { message: pending, goal: pendingGoal })
        : await change<{ message: Message }>({ operation: "send", message: pending });
      if (request !== generation) return;
      pending = null;
      pendingGoal = undefined;
      if (input.value.trim() === text) input.value = "";
      drafts.delete(id);
      sizeComposer();
      reply = null;
      updateReply();
      incoming(result.message);
      notice(t("messages.60823aaec7"));
    } catch (error) {
      notice(error);
    } finally {
      if (request === generation) {
        sendingGeneration = null;
        if (group) updateComposer(group);
        input.focus();
      }
    }
  };
}

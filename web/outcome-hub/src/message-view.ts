import { GoalEventKind, TaskEventKind } from "/assets/shared/contracts.js";
import { t, label } from "./i18n.js";
import { button, el } from "./dom.js";
import { avatar } from "./avatar.js";
import { display } from "./people.js";
import { contributionText } from "./contribution-text.js";
import { resultForMessage } from "./message-results.js";
import type { Message } from "./types.js";
import type { Goal } from "./goal-types.js";
function preview(message: Message): string {
  if (message.activity) return message.activity.has_content ? message.text : label("TaskEventKind", message.activity.kind);
  if (message.deleted) return t("deletion.deletedAnnouncement");
  if (message.event && message.sender_id !== message.event.person_id)
    return t("GoalEvent." + message.event.kind, { title: message.event.title, person: display(message.event.person_id || "operator"), attempt: message.event.attempt });
  return contributionText(message.text, display(message.sender_id));
}
export function messageRow(message: Message, history: Map<number, Message>, goals: Goal[], actions: { mention: (id: string) => void; reply: () => void; locate: (sequence: number) => void; review: (id: string) => void; remove?: () => void }): HTMLElement {
  if (message.activity) return activityRow(message, actions);
  const contribution = message.event?.kind === GoalEventKind.Submitted && message.sender_id === message.event.person_id;
  const submission = resultForMessage(message, goals);
  const author = submission?.work.person_id || message.sender_id;
  const system = message.event && !contribution && !submission;
  const row = el("article", undefined, "message" + (author === "operator" ? " own" : "") + (system ? " system-event" : "") + (contribution ? " agent-contribution" : ""));
  row.dataset.sequence = String(message.sequence);
  if (message.deleted) { row.append(el("p", t("deletion.deletedAnnouncement"))); return row; }
  if (system) {
    row.append(el("p", t("GoalEvent." + message.event!.kind, { title: message.event!.title, person: display(message.event!.person_id || "operator"), attempt: message.event!.attempt })));
    return row;
  }
  const face = button("", () => actions.mention(author));
  face.className = "avatar-button";
  face.setAttribute("aria-label", t("messages.mentionPerson", { name: display(author) }));
  face.append(avatar(author));
  if (author === "operator") face.disabled = true;
  const body = el("div", undefined, "message-body"), heading = el("header", undefined, "message-header");
  const sender = author === "operator" ? el("strong", display(author)) : button(`@${display(author)}`, () => actions.mention(author));
  sender.className = "message-sender";
  const date = new Date(message.accepted_ms), stamp = el("time", date.toLocaleTimeString("zh-CN", { hour: "2-digit", minute: "2-digit" }));
  stamp.dateTime = date.toISOString(); stamp.title = date.toLocaleString("zh-CN");
  heading.append(sender, stamp);
  const bubble = el("div", undefined, "chat-bubble");
  if (message.reply_to) {
    const parent = history.get(message.reply_to), quote = button("", () => actions.locate(message.reply_to!));
    quote.className = "message-quote";
    quote.disabled = !parent;
    quote.append(el("strong", parent ? `@${display(parent.sender_id)}` : t("messages.a40f9e2d59", { p0: message.reply_to })), el("span", parent ? preview(parent).replace(/\s+/gu, " ").slice(0, 120) : t("messages.previousPage")));
    bubble.append(quote);
  }
  const content = submission ? (submission.result.code ? label("WorkResultCode", submission.result.code) : submission.result.summary) : message.text;
  bubble.append(el("p", author === "operator" ? content : contributionText(content, display(author)), "message-text"));
  const footer = el("footer", undefined, "message-actions");
  if (submission) footer.append(el("span", label("WorkState", submission.work.state)));
  if (actions.remove) footer.append(button(t("deletion.action"), actions.remove));
  footer.append(button(t("messages.cf945e21dd"), actions.reply));
  if (contribution || submission) footer.append(button(t("messages.reviewResult"), () => actions.review(message.event!.goal_id)));
  body.append(heading, bubble, footer); row.append(face, body);
  return row;
}

function activityRow(message: Message, actions: { reply: () => void }): HTMLElement {
  const event = message.activity!, row = el("article", undefined, "message agent-contribution"), body = el("div", undefined, "message-body");
  row.dataset.sequence = String(message.sequence);
  const header = el("header", undefined, "message-header");
  header.append(el("strong", event.person_id ? display(event.person_id) : [TaskEventKind.Accepted, TaskEventKind.Returned, TaskEventKind.Supplemented, TaskEventKind.Cancelled].includes(event.kind as typeof TaskEventKind.Accepted) ? display("operator") : t("tasks.coordinator")), el("small", label("TaskEventKind", event.kind)));
  const content = el("div", undefined, "chat-bubble");
  content.append(el("strong", event.title));
  content.append(el("p", event.has_content ? message.text : t("tasks.event." + event.kind), "message-text"));
  const link = el("a", t("tasks.viewRun")); link.href = "#groups/" + encodeURIComponent(message.group_id) + "/tasks/" + encodeURIComponent(event.task_id);
  const footer = el("footer", undefined, "message-actions"); footer.append(button(t("messages.cf945e21dd"), actions.reply), link);
  body.append(header, content, footer); row.append(body); return row;
}

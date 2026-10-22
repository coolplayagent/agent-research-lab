import { GoalEventKind, GoalState, WorkState } from "/assets/shared/contracts.js";
import { researchProcess, supportsProcess } from "./research-process.js";
import { button, el } from "./dom.js";
import { label, t } from "./i18n.js";
import { display } from "./people.js";
import { resultForMessage } from "./message-results.js";
import { overdueWork, runningWork } from "./goal-selection.js";
import type { Goal } from "./goal-types.js";
import type { Message } from "./types.js";

interface TaskProgress {
  key: string;
  events: Message[];
  goal?: Goal;
}
type TimelineItem = { message: Message } | { progress: TaskProgress };

/** Compact structured updates without changing stored messages or member contributions. */
export function taskTimeline(messages: Message[], goals: Goal[]): TimelineItem[] {
  const timeline: TimelineItem[] = [], tasks = new Map<string, TaskProgress>();
  for (const message of [...messages].sort((a, b) => a.sequence - b.sequence)) {
    const event = message.event;
    if (event) {
      const key = JSON.stringify([message.group_id, event.goal_id]);
      let progress = tasks.get(key);
      if (!progress) {
        progress = { key, events: [], goal: goals.find(goal => goal.input.id === event.goal_id && goal.input.group_id === message.group_id) };
        tasks.set(key, progress);
        timeline.push({ progress });
      }
      progress.events.push(message);
      // Legacy submission events can still project their matching recorded result.
      if (event.kind !== GoalEventKind.Submitted && event.kind !== GoalEventKind.Failed) continue;
      if (message.sender_id !== event.person_id && !resultForMessage(message, goals)) continue;
    }
    timeline.push({ message });
  }
  return timeline;
}

function needsAttention(goal: Goal | undefined, now: number): boolean {
  return goal?.state === GoalState.Active && goal.work.some(work => work.state === WorkState.Failed || overdueWork(work, now));
}
export function progressSummary(goal: Goal | undefined, now = Date.now()): string {
  if (!goal) return t("progress.unavailable");
  if (goal.state !== GoalState.Active) return label("GoalState", goal.state);
  if (needsAttention(goal, now)) return t("progress.attention");
  if (goal.work.some(work => runningWork(work, now))) return t("progress.running");
  if (goal.work.some(work => work.state === WorkState.Ready)) return t("progress.waiting");
  if (goal.work.length && goal.work.every(work => work.state === WorkState.Submitted)) return t("progress.review");
  return t("progress.unavailable");
}
function timestamp(message: Message): HTMLTimeElement {
  const date = new Date(message.accepted_ms);
  const node = el("time", date.toLocaleTimeString("zh-CN", { hour: "2-digit", minute: "2-digit" }));
  node.dateTime = date.toISOString(); node.title = date.toLocaleString("zh-CN");
  return node;
}
export function progressCard(progress: TaskProgress, opened: boolean, review: (id: string) => void): HTMLElement {
  const { goal, events } = progress, first = events[0].event!, now = Date.now();
  const card = el("article", undefined, "task-progress");
  card.dataset.taskKey = progress.key;
  card.classList.toggle("needs-attention", needsAttention(goal, now));
  const header = el("header", undefined, "task-progress-header");
  header.append(el("span", t("progress.heading"), "task-progress-label"), el("span", progressSummary(goal, now), "task-progress-status"));
  card.append(header, el("strong", goal?.input.title || first.title, "task-progress-title"));
  if (goal) {
    const members = el("ul", undefined, "task-progress-members");
    for (const work of goal.work) {
      const member = el("li");
      member.dataset.workState = work.state;
      member.classList.toggle("needs-attention", goal.state === GoalState.Active && (work.state === WorkState.Failed || overdueWork(work, now)));
      const state = overdueWork(work, now) ? t("goals.58ada57084")
        : work.state === WorkState.Running && !runningWork(work, now) ? t("progress.unavailable")
        : goal.state === GoalState.Completed && work.state === WorkState.Submitted ? t("goals.788add54b2") : label("WorkState", work.state);
      member.append(el("span", display(work.person_id), "task-progress-person"), el("span", state, "task-progress-work"));
      if (work.attempt > 1) member.append(el("small", t("progress.attempt", { attempt: work.attempt })));
      if (supportsProcess(goal.input.scenario)) member.append(researchProcess(goal, work));
      members.append(member);
    }
    card.append(members);
    if (needsAttention(goal, now)) card.append(el("p", t("progress.help"), "task-progress-help"));
  }
  const details = el("details", undefined, "task-progress-history");
  details.dataset.progressKey = progress.key; details.open = opened;
  details.append(el("summary", t("progress.history", { count: events.length })));
  const history = el("ol");
  for (const message of events) {
    const event = message.event!, row = el("li");
    row.dataset.sequence = String(message.sequence);
    const description = el("span");
    if (event.person_id) description.append(el("strong", display(event.person_id)));
    description.append(el("span", label("GoalEventKind", event.kind)));
    if (event.attempt !== null && event.attempt > 1) description.append(el("small", t("progress.attempt", { attempt: event.attempt })));
    row.append(timestamp(message), description); history.append(row);
  }
  details.append(history);
  const action = button(t("progress.details"), () => review(first.goal_id));
  action.className = "task-progress-action";
  card.append(details, action);
  return card;
}

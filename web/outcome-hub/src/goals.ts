import { revealResearch } from "./workspace.js";
import { contributionText } from "./contribution-text.js";
import { currentGoal, overdueWork, runningWork } from "./goal-selection.js";
import { progressSummary } from "./task-progress.js";
import { GoalState, GroupKind, WorkState } from "/assets/shared/contracts.js";
import { t } from "./i18n.js";
import { label } from "./i18n.js";
import { api, query } from "./api.js";
import { button, el, get, notice } from "./dom.js";
import { display } from "./people.js";
import type { Group } from "./types.js";
import type { Goal, Work } from "./goal-types.js";
let selected: Group | null = null;
let snapshot: Goal[] = [];
let historySnapshot: Goal[] = [];
export function goalSnapshot(): Goal[] {
  return snapshot;
}
export function suspendGoals(): void {
  selected = null;
  generation++;
  window.clearTimeout(expiryTimer);
  snapshot = [];
  historySnapshot = [];
  get("goal-list").replaceChildren();
  get("more-goals").hidden = true;
  get("goal-summary").textContent = t("workspace.goalSummary");
}
export async function focusGoal(id: string): Promise<void> {
  const groupId = selected?.id;
  revealResearch("goals");
  while (selected?.id === groupId && !historySnapshot.some((goal) => goal.input.id === id) && next) {
    const cursor = next;
    await loadGoals(true);
    if (next === cursor) break;
  }
  if (selected?.id !== groupId) return;
  for (const node of get("goal-list").querySelectorAll<HTMLDetailsElement>(
    "[data-goal-id]",
  )) {
    if (node.dataset.goalId === id) {
      node.open = true;
      node.scrollIntoView({ block: "nearest" });
      return;
    }
  }
  notice(t("goals.notFound"));
}

let generation = 0;
let next: string | null = null;
let expiryTimer: number | undefined;
export function selectGoals(group: Group): void {
  suspendGoals();
  selected = group;
  get("goal-section").hidden = group.kind === GroupKind.Board;
  get<HTMLButtonElement>("new-goal").disabled = group.archived;
  void loadGoals();
}
async function mutate(
  goal: Goal,
  operation: string,
  work_id?: string,
): Promise<void> {
  try {
    await api("/api/v1/collaboration/goals", {
      operation,
      goal_id: goal.input.id,
      revision: goal.revision,
      ...(work_id ? { work_id } : {}),
    });
    await loadGoals();
  } catch (error) {
    notice(error);
  }
}
function expired(work: Work): boolean {
  return overdueWork(work);
}
function card(goal: Goal): HTMLDetailsElement {
  const details = el("details", undefined, "goal-card");
  details.dataset.goalId = goal.input.id;
  const summary = el("summary");
  summary.append(
    el("strong", goal.input.title),
    el("span", label("GoalState", goal.state), "badge"),
  );
  details.append(
    summary,
    el("p", goal.input.objective),
    el("p", t("goals.1459c102bb") + goal.input.acceptance),
    el(
      "small",
      t("goals.6ea45dffbd", {
        p0: label("Scenario", goal.input.scenario),
        p1: goal.input.max_seconds,
      }),
    ),
  );
  for (const work of goal.work) {
    const row = el("section", undefined, "work-item");
    row.append(
      el("strong", display(work.person_id)),
      el(
        "span",
        expired(work)
          ? t("goals.58ada57084")
          : work.state === WorkState.Running && !runningWork(work)
            ? t("progress.unavailable")
          : goal.state === GoalState.Completed &&
              work.state === WorkState.Submitted
            ? t("goals.788add54b2")
            : label("WorkState", work.state),
        "badge",
      ),
      el("p", work.instruction),
    );
    if (work.result) {
      row.append(
        el(
          "p",
          work.result.code
            ? label("WorkResultCode", work.result.code)
            : contributionText(work.result.summary, display(work.person_id)),
        ),
      );
      const evidence = el("details");
      evidence.dataset.evidenceKey = `${goal.input.id}/${work.id}/${work.attempt}`;
      evidence.append(
        el("summary", t("goals.da8098216f")),
        el("pre", JSON.stringify(work.result.evidence, null, 2)),
      );
      row.append(evidence);
    }
    if (
      goal.state === GoalState.Active &&
      (expired(work) ||
        work.state === WorkState.Submitted ||
        work.state === WorkState.Failed)
    )
      row.append(
        button(
          t("goals.bd7dd17c34"),
          () => mutate(goal, "retry", work.id),
        ),
      );
    details.append(row);
  }
  if (goal.state === GoalState.Active) {
    const actions = el("div", undefined, "actions");
    const accept = button(
      t("goals.9a341c73e2"),
      () => mutate(goal, "accept"),
    );
    accept.disabled = goal.work.some((w) => w.state !== WorkState.Submitted);
    const cancel = button(
      t("goals.a53a4e7bca"),
      () => mutate(goal, "cancel"),
    );
    cancel.disabled = goal.work.some(
      (w) => w.state === WorkState.Running && !expired(w),
    );
    actions.append(accept, cancel);
    details.append(actions);
  }
  return details;
}
export async function loadGoals(append = false): Promise<void> {
  if (!selected || selected.kind === GroupKind.Board) return;
  const groupId = selected.id,
    request = ++generation;
  try {
    const [current, history] = await Promise.all([
      api<{ goals: Goal[] }>("/api/v1/collaboration/goals?" + query({ group_id: groupId, current: "true" })),
      (async () => {
        const loaded: Goal[] = [];
        let cursor = append ? next || "" : "";
        const target = append ? 0 : historySnapshot.length;
        do {
          const page = await api<{ goals: Goal[]; next_after: string | null }>(
            "/api/v1/collaboration/goals?" + query({ group_id: groupId, after: cursor }),
          );
          if (generation !== request || selected?.id !== groupId) return { loaded, cursor };
          loaded.push(...page.goals);
          const following = page.next_after || "";
          if (following === cursor) { cursor = ""; break; }
          cursor = following;
        } while (!append && cursor && loaded.length < target);
        return { loaded, cursor };
      })(),
    ]);
    if (generation !== request || selected?.id !== groupId) return;
    const { loaded, cursor } = history;
    historySnapshot = append ? [...historySnapshot, ...loaded] : loaded;
    const combined = new Map(historySnapshot.map(goal => [goal.input.id, goal]));
    for (const goal of current.goals) {
      const known = combined.get(goal.input.id);
      if (!known || known.revision <= goal.revision) combined.set(goal.input.id, goal);
    }
    snapshot = [...combined.values()];
    window.clearTimeout(expiryTimer);
    const now = Date.now();
    const deadlines = snapshot.filter(goal => goal.state === GoalState.Active).flatMap((g) =>
      g.work
        .filter(work => runningWork(work, now))
        .map((w) => w.deadline_ms!),
    );
    if (deadlines.length)
      expiryTimer = window.setTimeout(
        () => void loadGoals(),
        Math.max(1, Math.min(...deadlines) - now + 50),
      );
    const active = currentGoal(snapshot, now);
    get("goal-summary").textContent = active
      ? t(active.state === GoalState.Active ? "workspace.currentGoal" : "workspace.latestGoal", {
          title: active.input.title, status: progressSummary(active, now),
        })
      : t("workspace.goalSummary");
    document.dispatchEvent(new Event("workspace-goals"));
    const list = get("goal-list");
    const opened = new Set(
      [
        ...list.querySelectorAll<HTMLDetailsElement>(
          "details[open][data-goal-id]",
        ),
      ].map((d) => d.dataset.goalId),
    );
    const evidenceOpened = new Set(
      [...list.querySelectorAll<HTMLDetailsElement>("details[open][data-evidence-key]")]
        .map((node) => node.dataset.evidenceKey),
    );
    list.replaceChildren();
    for (const goal of historySnapshot) {
      const node = card(combined.get(goal.input.id)!);
      node.open = opened.has(goal.input.id);
      for (const evidence of node.querySelectorAll<HTMLDetailsElement>("[data-evidence-key]"))
        evidence.open = evidenceOpened.has(evidence.dataset.evidenceKey);
      list.append(node);
    }
    if (!list.childElementCount)
      list.append(el("p", t("goals.1bc752e12f"), "goal-empty"));
    next = cursor || null;
    get("more-goals").hidden = !next;
    get("more-goals").onclick = () => void loadGoals(true);
  } catch (error) {
    if (generation === request) notice(error);
  }
}

import { GoalState, GroupKind, WorkState } from "/assets/shared/contracts.js";
import { t } from "./i18n.js";
import { label } from "./i18n.js";
import { api, query } from "./api.js";
import { button, el, get, notice } from "./dom.js";
import { display } from "./people.js";
import type { Group } from "./types.js";
import type { Goal, Work } from "./goal-types.js";
let selected: Group | null = null;
let generation = 0;
let next: string | null = null;
let expiryTimer: number | undefined;
export function selectGoals(group: Group): void {
  window.clearTimeout(expiryTimer);
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
    await api("/api/im/goals", {
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
  return (
    work.state === WorkState.Running &&
    work.deadline_ms !== null &&
    work.deadline_ms <= Date.now()
  );
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
        p0:
          goal.input.scenario === "research"
            ? t("goals.70596ea10b")
            : t("goals.0ad442f5f7"),
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
          goal.input.scenario === "research" && work.result.code
            ? label("WorkResultCode", work.result.code)
            : work.result.summary,
        ),
      );
      const evidence = el("details");
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
          () => void mutate(goal, "retry", work.id),
        ),
      );
    details.append(row);
  }
  if (goal.state === GoalState.Active) {
    const actions = el("div", undefined, "actions");
    const accept = button(
      t("goals.9a341c73e2"),
      () => void mutate(goal, "accept"),
    );
    accept.disabled = goal.work.some((w) => w.state !== WorkState.Submitted);
    const cancel = button(
      t("goals.a53a4e7bca"),
      () => void mutate(goal, "cancel"),
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
    const page = await api<{
      goals: Goal[];
      next_after: string | null;
    }>(
      "/api/im/goals?" +
        query({ group_id: groupId, after: append ? next || "" : "" }),
    );
    if (generation !== request || selected.id !== groupId) return;
    window.clearTimeout(expiryTimer);
    const deadlines = page.goals.flatMap((g) =>
      g.work
        .filter(
          (w) =>
            w.state === WorkState.Running &&
            w.deadline_ms !== null &&
            w.deadline_ms > Date.now(),
        )
        .map((w) => w.deadline_ms!),
    );
    if (deadlines.length)
      expiryTimer = window.setTimeout(
        () => void loadGoals(),
        Math.max(1, Math.min(...deadlines) - Date.now() + 50),
      );
    const list = get("goal-list");
    const opened = new Set(
      [
        ...list.querySelectorAll<HTMLDetailsElement>(
          "details[open][data-goal-id]",
        ),
      ].map((d) => d.dataset.goalId),
    );
    if (!append) list.replaceChildren();
    for (const goal of page.goals) {
      const node = card(goal);
      node.open = opened.has(goal.input.id);
      list.append(node);
    }
    if (!list.childElementCount)
      list.append(el("p", t("goals.1bc752e12f"), "goal-empty"));
    next = page.next_after;
    get("more-goals").hidden = !next;
    get("more-goals").onclick = () => void loadGoals(true);
  } catch (error) {
    if (generation === request) notice(error);
  }
}

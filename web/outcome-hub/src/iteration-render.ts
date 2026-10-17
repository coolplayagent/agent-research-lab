import { ActivationState, IterationState } from "/assets/shared/contracts.js";
import { button, el } from "./dom.js";
import { t, label } from "./i18n.js";
import { focusGoal } from "./goals.js";
import { display } from "./people.js";
import type { IterationRun, IterationRound } from "./iteration-types.js";

export function iterationCard(run: IterationRun, open: () => void): HTMLElement {
  const card = el("article", undefined, "card iteration-card");
  card.dataset.iterationId = run.input.id;
  card.append(
    button(run.input.title, open),
    el("p", label("Scenario", run.input.scenario_id)),
    el("p", label("IterationState", run.state)),
    el("small", t("iterations.progress", {
      rounds: run.rounds.length,
      max: run.input.max_rounds,
      used: run.used_seconds,
      budget: run.input.total_budget_seconds,
    })),
  );
  return card;
}

function evidence(value: unknown): HTMLElement {
  const details = el("details");
  details.append(
    el("summary", t("research.evidence")),
    el("pre", JSON.stringify(value, null, 2)),
  );
  return details;
}

function roundCard(round: IterationRound): HTMLElement {
  const card = el("article", undefined, "card iteration-round");
  card.append(
    el("h4", t("iterations.round", { number: round.number })),
    el("p", label("RoundState", round.state)),
    button(t("iterations.openGoal"), () => focusGoal(round.goal_id)),
    el("small", round.goal_id),
  );
  const values = el("dl", undefined, "iteration-facts");
  values.append(
    el("dt", t("iterations.baseline")),
    el("dd", round.baseline_version_id),
    el("dt", t("iterations.candidate")),
    el("dd", round.candidate
      ? `${round.candidate.version_id} · ${label("CandidateState", round.candidate.state)}`
      : t("iterations.noCandidate")),
    el("dt", t("iterations.evaluation")),
    el("dd", round.evaluation
      ? label("EvaluationState", round.evaluation.state)
      : t("iterations.unassessed")),
    el("dt", t("iterations.activation")),
    el("dd", round.activation
      ? label("ActivationState", round.activation.state)
      : t("iterations.notActivated")),
  );
  card.append(values);
  if (round.evaluation) {
    card.append(el("p", round.evaluation.summary));
    const section = el("section", undefined, "iteration-evidence");
    section.append(el("h5", t("iterations.evaluation")), evidence({
      evaluator_work_id: round.evaluation.evaluator_work_id,
      goal_revision: round.evaluation.goal_revision,
      goal_attempts: round.goal_attempts,
      evidence: round.evaluation.evidence,
    }));
    card.append(section);
  }
  if (round.activation) {
    const section = el("section", undefined, "iteration-evidence");
    section.append(
      el("h5", t("iterations.activation")),
      el("code", round.activation.reference),
      evidence(round.activation.evidence),
    );
    card.append(section);
  }
  if (round.feedback) card.append(el("p", round.feedback, "iteration-feedback"));
  return card;
}

export function iterationDetail(
  run: IterationRun,
  pause: () => Promise<void>,
  resume: () => Promise<void>,
  stop: () => Promise<void>,
  graph: () => void,
  review?: () => Promise<void>,
): HTMLElement[] {
  const controls = el("div", undefined, "toolbar");
  const switching = run.rounds.at(-1)?.activation?.state === ActivationState.Pending;
  const control = (key: string, action: () => Promise<void>) => {
    const node = button(t(key), action);
    node.disabled = switching;
    return node;
  };
  if (run.state === IterationState.Running)
    controls.append(control("iterations.pause", pause));
  if (run.state === IterationState.Paused)
    controls.append(control("iterations.resume", resume));
  if (run.state === IterationState.Running || run.state === IterationState.Paused)
    controls.append(control("iterations.stop", stop));
  if (switching) controls.append(el("p", t("iterations.activationPending")));
  controls.append(button(t("workspace.evolution"), graph));
  if (review && !switching && (run.state === IterationState.Running || run.state === IterationState.Paused) && run.rounds.at(-1)?.finished_ms === null)
    controls.append(button(t("iterations.review"), review));
  const assignments = el("div", undefined, "iteration-assignments");
  for (const assignment of run.input.assignments) {
    const item = el("details");
    item.append(el("summary", display(assignment.person_id)), el("p", assignment.instruction));
    assignments.append(item);
  }
  return [
    el("h3", run.input.title),
    el("p", `${label("Scenario", run.input.scenario_id)} · ${label("IterationState", run.state)}`),
    el("p", run.input.objective),
    el("h4", t("iterations.acceptance")),
    el("p", run.input.acceptance),
    el("small", t("iterations.progress", {
      rounds: run.rounds.length,
      max: run.input.max_rounds,
      used: run.used_seconds,
      budget: run.input.total_budget_seconds,
    })),
    controls,
    assignments,
    ...[...run.rounds].reverse().map(roundCard),
  ];
}

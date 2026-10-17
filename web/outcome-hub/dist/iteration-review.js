import { CandidateState, EvaluationState, RoundState, WorkState } from "/assets/shared/contracts.js";
import { api, query } from "./api.js";
import { button, el } from "./dom.js";
import { area, dialog, field, finish, input, select } from "./forms.js";
import { label, t } from "./i18n.js";
import { display } from "./people.js";
export async function reviewIteration(run, save) {
    const round = run.rounds.at(-1);
    if (!round || round.finished_ms !== null)
        return;
    const goal = await roundGoal(run.input.group_id, round.goal_id);
    if (goal.work.some(work => round.goal_attempts[work.id] !== work.attempt))
        throw new Error(t("iterations.attemptChanged"));
    const submitted = goal.work.filter(work => work.state === WorkState.Submitted && work.result?.succeeded);
    const box = dialog(t("iterations.review")), feedback = area(round.feedback);
    feedback.maxLength = 2000;
    box.form.append(el("p", run.input.title), el("p", t("iterations.reviewNote")));
    const actions = el("div", undefined, "toolbar");
    const perform = async (action) => {
        await save(action);
        box.dialog.close();
    };
    if (!round.candidate && submitted.length) {
        const version = input(""), producer = select(submitted.map(work => [work.id, display(work.person_id)]), submitted[0].id);
        box.form.append(el("p", t("iterations.candidateNote")), field(t("iterations.candidateVersion"), version), field(t("iterations.producer"), producer));
        actions.append(button(t("iterations.recordCandidate"), () => perform({ operation: "record_candidate", version_id: version.value.trim(), producer_work_id: producer.value })));
    }
    if (round.candidate && !round.evaluation) {
        const producer = goal.work.find(work => work.id === round.candidate.producer_work_id);
        const evaluators = submitted.filter(work => work.person_id !== producer?.person_id);
        const evaluator = select(evaluators.map(work => [work.id, display(work.person_id)]), evaluators[0]?.id || "");
        const verdict = select(Object.values(EvaluationState).map(state => [state, label("EvaluationState", state)]), EvaluationState.Failed);
        const summary = area("");
        summary.maxLength = 2000;
        box.form.append(field(t("iterations.evaluator"), evaluator), field(t("iterations.evaluation"), verdict), field(t("iterations.evaluationSummary"), summary));
        const evaluate = button(t("iterations.recordEvaluation"), async () => {
            const work = evaluators.find(work => work.id === evaluator.value);
            if (!work?.result)
                throw new Error(t("iterations.needEvaluator"));
            await perform({ operation: "evaluate", evaluation: {
                    evaluator_work_id: work.id, goal_revision: goal.revision,
                    state: verdict.value, summary: summary.value.trim(), evidence: work.result.evidence,
                } });
        });
        evaluate.disabled = !evaluators.length;
        actions.append(evaluate);
    }
    if (round.candidate?.state === CandidateState.Proposed && round.evaluation?.state === EvaluationState.Passed)
        actions.append(button(t("iterations.accept"), () => perform({ operation: "accept" })));
    box.form.append(actions, field(t("iterations.feedback"), feedback));
    const ending = select([
        [RoundState.NoImprovement, label("RoundState", RoundState.NoImprovement)],
        [RoundState.Failed, label("RoundState", RoundState.Failed)],
        ...(round.candidate?.state === CandidateState.Accepted ? [[RoundState.Completed, label("RoundState", RoundState.Completed)]] : []),
    ], round.candidate?.state === CandidateState.Accepted ? RoundState.Completed : RoundState.NoImprovement);
    box.form.append(field(t("iterations.finishState"), ending));
    finish(box, () => perform({ operation: "finish_round", state: ending.value, feedback: feedback.value.trim() }));
    box.form.querySelector("button[type='submit']").textContent = t("iterations.finishRound");
}
async function roundGoal(groupId, goalId) {
    let after = "", count = 0;
    do {
        const page = await api("/api/v1/collaboration/goals?" + query({ group_id: groupId, after }));
        const goal = page.goals.find(goal => goal.input.id === goalId && goal.input.group_id === groupId);
        if (goal)
            return goal;
        count += page.goals.length;
        if (page.next_after && (page.next_after === after || count >= 10000))
            break;
        after = page.next_after || "";
    } while (after);
    throw new Error(t("iterations.goalMissing"));
}

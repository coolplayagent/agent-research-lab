import { GoalState, WorkState } from "/assets/shared/contracts.js";
export function runningWork(work, now = Date.now()) {
    return work.state === WorkState.Running && work.deadline_ms !== null &&
        Number.isFinite(work.deadline_ms) && work.deadline_ms > now;
}
export function overdueWork(work, now = Date.now()) {
    return work.state === WorkState.Running && work.deadline_ms !== null &&
        Number.isFinite(work.deadline_ms) && work.deadline_ms <= now;
}
function priority(goal, now) {
    if (goal.state !== GoalState.Active)
        return 0;
    if (goal.work.some(work => runningWork(work, now)))
        return 3;
    if (goal.work.some(work => work.state === WorkState.Ready))
        return 2;
    return 1;
}
/** Creation time, rather than an opaque ID or revision, breaks activity ties. */
export function currentGoal(goals, now = Date.now()) {
    return [...goals].sort((a, b) => priority(b, now) - priority(a, now) ||
        (Number.isFinite(b.created_ms) ? b.created_ms : 0) -
            (Number.isFinite(a.created_ms) ? a.created_ms : 0) ||
        (a.input.id < b.input.id ? -1 : a.input.id > b.input.id ? 1 : 0))[0];
}
/** A member may be executing a different concurrent goal from the group summary. */
export function memberRunningWork(goals, personId, now = Date.now()) {
    const goal = currentGoal(goals.filter(goal => goal.state === GoalState.Active &&
        goal.work.some(work => work.person_id === personId && runningWork(work, now))), now);
    return goal?.work.find(work => work.person_id === personId && runningWork(work, now));
}

import { GoalEventKind, WorkState } from "/assets/shared/contracts.js";
/** Attach only the recorded result for this event's group, person and attempt. */
export function resultForMessage(message, goals) {
    const event = message.event;
    if (!event || (event.kind !== GoalEventKind.Submitted && event.kind !== GoalEventKind.Failed))
        return null;
    const goal = goals.find((goal) => goal.input.id === event.goal_id && goal.input.group_id === message.group_id);
    const work = goal?.work.find((work) => work.id === event.work_id && work.person_id === event.person_id && work.attempt === event.attempt);
    if (!goal || !work?.result)
        return null;
    if (event.kind === GoalEventKind.Submitted && (work.state !== WorkState.Submitted || !work.result.succeeded))
        return null;
    if (event.kind === GoalEventKind.Failed && (work.state !== WorkState.Failed || work.result.succeeded))
        return null;
    return { goal, work, result: work.result };
}

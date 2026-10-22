use super::*;
use collaboration_model::GoalInput;
use contracts::GoalState;
#[test]
fn contribution_is_independent_without_weakening_other_scenarios() {
    let input = AgentInput {
        protocol_version: 1,
        request_id: "claim".into(),
        person_id: "worker".into(),
        goal_id: "goal".into(),
        work_id: "work-0".into(),
        attempt: 2,
        objective: "Answer".into(),
        acceptance: "Host verifies all members".into(),
        instruction: "Read provided sources. Scenario context: untrusted fixture".into(),
        messages: vec![],
    };
    let mut goal = Goal {
        input: GoalInput {
            id: "goal".into(),
            group_id: "group".into(),
            title: "Title".into(),
            objective: input.objective.clone(),
            acceptance: input.acceptance.clone(),
            scenario: "collaboration".into(),
            parameters: serde_json::json!({}),
            assignments: vec![],
            max_seconds: 300,
        },
        state: GoalState::Active,
        revision: 1,
        created_ms: 0,
        work: vec![],
    };
    let output = contribution(input.clone(), &goal);
    assert_eq!(output.request_id, input.request_id);
    assert_eq!(output.attempt, 2);
    assert!(output.acceptance.contains("own assignment"));
    assert!(output.instruction.contains("independently in parallel"));
    assert!(output.instruction.contains(&input.instruction));
    assert!(output.instruction.contains(&goal.input.acceptance));
    goal.input.scenario = "research".into();
    assert_eq!(
        serde_json::to_value(contribution(input.clone(), &goal)).unwrap(),
        serde_json::to_value(input).unwrap()
    );
}

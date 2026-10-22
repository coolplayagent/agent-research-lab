//! Separate an individual contribution from host-owned acceptance of the whole goal.
use collaboration_model::Goal;
use service_api::AgentInput;
pub(crate) fn contribution(mut input: AgentInput, goal: &Goal) -> AgentInput {
    if goal.input.scenario == "collaboration" {
        input.acceptance = "Submit your own substantive answer to the objective, with supporting evidence where available and explicit limitations. Set succeeded according to completion of your own assignment. The host independently reviews the overall goal; pending host acceptance or other members' answers does not make your completed contribution fail.".into();
        input.instruction = format!(
            "{}\n\nExecution responsibilities: members answer independently in parallel. Use existing reports only when provided; do not wait for future replies or invent agreement. Return your contribution in summary. OutcomeHub persists and displays this response in the conversation after validating the current attempt; you do not need a messaging tool or a delivery receipt. Do not claim host acceptance. Overall goal criteria (for the host): {}",
            input.instruction, goal.input.acceptance
        );
    }
    input
}
#[cfg(test)]
#[path = "execution_input_tests.rs"]
mod tests;

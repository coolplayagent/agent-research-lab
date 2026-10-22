//! Golden bytes captured by executing the pre-refactor types at commit 4fd42de.
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;

fn wire<T: DeserializeOwned + Serialize>(input: &Value) -> String {
    serde_json::to_string(&serde_json::from_value::<T>(input.clone()).unwrap()).unwrap()
}

#[test]
fn extracted_contracts_keep_original_wire_bytes_and_defaults() {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/wire-4fd42de.json")).unwrap();
    assert_eq!(
        fixture["source_commit"],
        "4fd42de092b8af96913c7df64de0c0b82e9123d0"
    );
    let records = fixture["records"].as_array().unwrap();
    assert_eq!(records.len(), 14);
    for record in records {
        let input = &record["input"];
        let actual = match record["kind"].as_str().unwrap() {
            "message" => wire::<collaboration_model::Message>(input),
            "group" => wire::<collaboration_model::NewGroup>(input),
            "goal" => wire::<collaboration_model::GoalInput>(input),
            "outcome_command" => wire::<collaboration_model::OutcomeCommand>(input),
            "coordination" => wire::<collaboration_model::ResearchChange>(input),
            "storage_request" => wire::<collaboration_client::Request>(input),
            "agent_input" => wire::<service_api::AgentInput>(input),
            "configuration" => wire::<service_api::Configuration>(input),
            unknown => panic!("unhandled fixture kind: {unknown}"),
        };
        assert_eq!(
            actual,
            record["wire"].as_str().unwrap(),
            "{}",
            record["kind"]
        );
    }
}

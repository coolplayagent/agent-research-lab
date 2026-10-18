use super::*;
use crate::tests::completion_fixture;
use std::path::Path;

fn publish_receipt(authority: &Path, receipt: &Value) {
    let mut state: Value = storage::read(authority).unwrap();
    state["result"]["frames"]["1"]["nodes"]["task"]["outputs"]["result"] =
        json!(serde_json::to_string(receipt).unwrap());
    storage::write(authority, &state).unwrap();
}

#[test]
fn completed_result_checks_frozen_pins_and_ignores_uncommitted_worker_files() {
    let (temp, config, job, _) = completion_fixture(false, json!([]));
    let committed = completed_result(&config, &job).unwrap();
    storage::write(
        &config
            .state_dir
            .join("runs")
            .join(&job.run_id)
            .join("receipt.json"),
        &json!({"agent_report":{"summary":"Uncommitted worker claim"}}),
    )
    .unwrap();
    assert_eq!(completed_result(&config, &job).unwrap(), committed);

    let authority = temp.path().join("authority.json");
    for field in [
        "receipt_protocol",
        "config_digest",
        "source_commit",
        "superpod_commit",
        "prompt_digest",
        "model",
        "research_inputs",
        "backend",
    ] {
        let mut changed = committed.clone();
        changed[field] = json!("changed");
        publish_receipt(&authority, &changed);
        assert!(
            completed_result(&config, &job).is_err(),
            "accepted changed {field}"
        );
    }
    publish_receipt(&authority, &committed);
    assert_eq!(completed_result(&config, &job).unwrap(), committed);
}

#[test]
fn completed_result_requires_known_success_even_with_a_valid_receipt() {
    let (temp, config, job, _) = completion_fixture(false, json!([]));
    let authority = temp.path().join("authority.json");
    let mut state: Value = storage::read(&authority).unwrap();
    for status in [
        json!(WorkflowState::Running),
        json!(WorkflowState::Failed),
        json!(WorkflowState::Cancelled),
        json!(WorkflowState::Unknown),
        json!("future_protocol_state"),
        Value::Null,
    ] {
        state["result"]["status"] = status.clone();
        storage::write(&authority, &state).unwrap();
        assert!(
            completed_result(&config, &job).is_err(),
            "accepted {status}"
        );
    }
    state["result"]["status"] = json!(WorkflowState::Succeeded);
    storage::write(&authority, &state).unwrap();
    assert!(completed_result(&config, &job).is_ok());
}

#[test]
fn dependency_reports_require_local_committed_agreement_and_frozen_bindings() {
    let (temp, config, parent, owner) = completion_fixture(false, json!([]));
    let receipt = completed_result(&config, &parent).unwrap();
    let local = config
        .state_dir
        .join("runs")
        .join(&parent.run_id)
        .join("receipt.json");
    let authority = temp.path().join("authority.json");
    let mut child = parent.clone();
    child.task.id = "dependent-review".into();
    child.task.dependencies = vec![parent.task.id.clone()];
    storage::write(&local, &receipt).unwrap();
    assert_eq!(
        completed_dependencies(&config, &owner, &child).unwrap()[0]["receipt"],
        receipt
    );

    let mut forged = receipt.clone();
    forged["agent_report"]["summary"] = json!("Replaced after completion");
    storage::write(&local, &forged).unwrap();
    assert!(committed_receipt(&config, &owner, &parent).is_err());
    assert!(completed_dependencies(&config, &owner, &child).is_err());

    for field in ["source_commit", "superpod_commit", "prompt_digest"] {
        let mut changed = receipt.clone();
        changed[field] = json!("different-frozen-input");
        storage::write(&local, &changed).unwrap();
        publish_receipt(&authority, &changed);
        assert!(
            completed_dependencies(&config, &owner, &child).is_err(),
            "accepted changed {field}"
        );
    }
    storage::write(&local, &receipt).unwrap();
    publish_receipt(&authority, &receipt);
    assert!(completed_dependencies(&config, &owner, &child).is_ok());
}

#[test]
fn report_schema_rejects_false_evidence_types() {
    assert!(
        validate_report(&json!({"summary":"ok","findings":[false],"sources":[],"limitations":[]}))
            .is_err()
    );
    assert!(
        validate_report(&json!({"summary":"ok","findings":[],"sources":[],"limitations":[]}))
            .is_ok()
    );
}

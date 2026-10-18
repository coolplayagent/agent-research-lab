use super::*;
use crate::{
    jobs, receipts::authoritative_success_receipt, rendered_experiment_prompt,
    tests::completion_fixture, workflow,
};
use native_effects::fs;
use std::path::PathBuf;

#[test]
fn gui_followup_binds_explicit_tools_and_rejects_invalid_siblings() {
    let (_temp, mut c, job, w) = completion_fixture(
        false,
        json!([
            {"repository":"superpod","role":"review","prompt":"unknown tool","write":false,"exploratory":false,"required_tools":["unknown-cli"]},
            {"repository":"superpod","role":"research","prompt":"observe the owned GUI and sandbox evidence","write":false,"exploratory":true,"required_tools":["computer-use-cli","repo-sandbox"]},
            {"repository":"superpod","role":"review","prompt":"duplicate binding","write":false,"exploratory":false,"required_tools":["computer-use-cli","computer-use-cli"]}
        ]),
    );
    // Keep the unused generated transport executable busy throughout the positive case.
    // This models a writable executable, not an owner transaction or the historic holder.
    let private_script_writer = fs::OpenOptions::new()
        .write(true)
        .open(&c.workflow)
        .unwrap();
    let busy_error = w.status(&job.run_id).unwrap_err();
    let io_error = busy_error
        .chain()
        .find_map(|cause| cause.downcast_ref::<std::io::Error>())
        .expect("the generated executable must fail with an I/O error");
    assert_eq!(io_error.raw_os_error(), Some(26));
    let fixture = if let Some(runfiles) = std::env::var_os("TEST_SRCDIR") {
        PathBuf::from(runfiles)
            .join(std::env::var_os("TEST_WORKSPACE").expect("Bazel test workspace"))
            .join("crates/execution-runtime/fixtures/completion-workflow.sh")
    } else {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/completion-workflow.sh")
    };
    c.workflow = fixture.canonicalize().unwrap();
    let w = workflow(&c);
    let binding = c.tools["relay-memory"].clone();
    c.tools.insert("computer-use-cli".into(), binding.clone());
    c.tools.insert("repo-sandbox".into(), binding);
    let receipt = authoritative_success_receipt(&w, &job).unwrap();
    let dir = c.state_dir.join("runs").join(&job.run_id);
    let report = admit_followups(&c, &job, &dir, &receipt).unwrap();
    assert_eq!(report.queued.len(), 1);
    assert_eq!(
        report
            .rejected
            .iter()
            .map(|item| item.index)
            .collect::<Vec<_>>(),
        vec![0, 2]
    );
    assert!(report.rejected[0].reason.contains("unknown required tool"));
    assert!(
        report.rejected[1]
            .reason
            .contains("duplicate required tool")
    );
    let child: Job = storage::read(&job_path(&c, &report.queued[0])).unwrap();
    assert_eq!(
        child.task.required_tools,
        ["computer-use-cli", "repo-sandbox"]
    );
    assert!(!child.task.write);
    assert!(
        rendered_experiment_prompt(&c, &child.task, None)
            .unwrap()
            .contains("Owned target experiment")
    );
    let schema = report_schema(&c, &job.task);
    for branch in schema["properties"]["next_tasks"]["items"]["anyOf"]
        .as_array()
        .unwrap()
    {
        assert!(
            branch["required"]
                .as_array()
                .unwrap()
                .contains(&json!("required_tools"))
        );
        assert_eq!(branch["properties"]["required_tools"]["maxItems"], 7);
        assert_eq!(
            branch["properties"]["required_tools"]["items"]["enum"],
            json!(["computer-use-cli", "relay-memory", "repo-sandbox"])
        );
    }
    let replay = admit_followups(&c, &job, &dir, &receipt).unwrap();
    assert_eq!(replay.queued, report.queued);
    assert_eq!(jobs(&c).unwrap().len(), 2);
    drop(private_script_writer);
}

#[test]
fn followup_authority_depth_and_schema_are_consistent() {
    let (_temp, c, mut job, w) = completion_fixture(
        false,
        json!([
            {"repository":"superpod","role":"review","prompt":"independent review","write":false,"exploratory":false},
            {"repository":"superpod","role":"implement","prompt":"nested implementation","write":true,"exploratory":false}
        ]),
    );
    let receipt = authoritative_success_receipt(&w, &job).unwrap();
    let dir = c.state_dir.join("runs").join(&job.run_id);
    job.task.depth = 3;
    let report = admit_followups(&c, &job, &dir, &receipt).unwrap();
    assert!(report.queued.is_empty());
    assert_eq!(report.rejected.len(), 2);
    assert_eq!(
        report_schema(&c, &job.task)["properties"]["next_tasks"]["maxItems"],
        0
    );
    job.task.depth = 0;
    job.task.id = "independent-research".into();
    let report = admit_followups(&c, &job, &dir, &receipt).unwrap();
    assert!(report.queued.is_empty());
    assert_eq!(report.rejected.len(), 2);
    assert!(followup_instructions(&job.task).contains("next_tasks must be []"));
    assert_eq!(
        report_schema(&c, &job.task)["properties"]["next_tasks"]["maxItems"],
        0
    );
    // Implementation receives review authority only, even if its ID happens
    // to carry the synthesis prefix.
    job.task.id = "synthesis-recovery".into();
    job.task.role = "implement".into();
    let report = admit_followups(&c, &job, &dir, &receipt).unwrap();
    assert_eq!(report.queued.len(), 1);
    assert_eq!(report.rejected[0].index, 1);
    let items = report_schema(&c, &job.task)["properties"]["next_tasks"]["items"].clone();
    assert_eq!(items["properties"]["role"]["enum"], json!(["review"]));
    assert_eq!(items["properties"]["write"]["enum"], json!([false]));
    let proposal: NextTask =
        serde_json::from_value(receipt["agent_report"]["next_tasks"][0].clone()).unwrap();
    assert!(validate_proposal(&c, &job.task, 3, &proposal).is_err());
}

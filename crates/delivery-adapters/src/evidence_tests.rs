use super::*;
use crate::{package::file_sha256, tests::evidence};

#[test]
fn booleans_do_not_override_tampered_or_stale_artifacts() {
    let temp = tempfile::tempdir().unwrap();
    let mut evidence = evidence(temp.path());
    evidence.validate().unwrap();
    fs::write(&evidence.qualitygate_report.path, "{}").unwrap();
    assert!(evidence.validate().is_err());
    evidence.qualitygate_report.sha256 = file_sha256(&evidence.qualitygate_report.path).unwrap();
    assert!(evidence.validate().is_err());
}

#[test]
fn historical_evidence_can_be_read_but_cannot_pass_new_release_gates() {
    let temp = tempfile::tempdir().unwrap();
    let mut old = serde_json::to_value(evidence(temp.path())).unwrap();
    old.as_object_mut().unwrap().remove("upstream");
    let old: ReleaseEvidence = serde_json::from_value(old).unwrap();
    assert!(old.upstream.is_none());
    assert!(old.validate().is_err());
    let mut without_skills = serde_json::to_value(evidence(temp.path())).unwrap();
    without_skills
        .as_object_mut()
        .unwrap()
        .remove("skills_manifest");
    let without_skills: ReleaseEvidence = serde_json::from_value(without_skills).unwrap();
    assert!(without_skills.validate().is_err());
    let mut mismatched = evidence(temp.path());
    mismatched.upstream.as_mut().unwrap().commit = "1".repeat(40);
    assert!(mismatched.validate().is_err());
}

#[test]
fn delivery_gate_preserves_legacy_scopes_and_rejects_unknown_or_partial_states() {
    use crate::tests::artifact;
    use serde_json::json;
    let temp = tempfile::tempdir().unwrap();
    let original = evidence(temp.path());
    let valid = original.qualitygate_report.read().unwrap();
    let mut cases = Vec::new();
    for scope in ["repository", "delivery", "task"] {
        let mut report = valid.clone();
        report["scope"] = scope.into();
        cases.push((report, true));
    }
    for (pointer, value) in [
        ("/scope", json!("path")),
        ("/scope", json!("future")),
        ("/profile", json!("quick")),
        ("/profile", json!(null)),
        ("/gate/decision", json!("incomplete")),
        ("/gate/decision", json!("fail")),
        ("/gate/decision", json!("future")),
        ("/gate/complete", json!(false)),
        ("/plan/pending_delivery_checks", json!(["tests"])),
    ] {
        let mut report = valid.clone();
        *report.pointer_mut(pointer).unwrap() = value;
        cases.push((report, false));
    }
    for (report, accepted) in cases {
        let mut bound = original.clone();
        bound.qualitygate_report = artifact(temp.path(), "case-qualitygate.json", report);
        let mut snapshot = original.snapshot_receipt.read().unwrap();
        snapshot["qualitygate_report_sha256"] = bound.qualitygate_report.sha256.clone().into();
        bound.snapshot_receipt = artifact(temp.path(), "case-snapshot.json", snapshot);
        assert_eq!(bound.validate().is_ok(), accepted);
    }
}

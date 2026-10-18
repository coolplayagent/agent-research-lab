use super::verify_source_qualitygate;
use crate::{SkillSnapshot, SourceQualitygate};
use serde_json::{Value, json};
use std::{fs, path::Path};

fn skill_with_report(root: &Path, report: &Value) -> SkillSnapshot {
    let path = root.join("gate.json");
    storage::write(&path, report).unwrap();
    SkillSnapshot {
        name: "source-skill".into(),
        repository: "coolplayagent/example".into(),
        installed_path: root.join("installed"),
        runtime_path: root.join("runtime"),
        tree_sha256: "b".repeat(64),
        runtime_sha256: "c".repeat(64),
        release_tag: None,
        release_id: None,
        source_commit: Some("a".repeat(40)),
        source_qualitygate: Some(SourceQualitygate {
            sha256: storage::digest(&fs::read(&path).unwrap()),
            path,
        }),
        checked_at: "2026-10-09T00:00:00Z".into(),
    }
}

#[test]
fn source_override_requires_every_full_diff_gate_condition() {
    let temp = tempfile::tempdir().unwrap();
    let report = json!({
        "profile":"full", "scope":"delivery", "gate":{"complete":true,"decision":"pass"},
        "plan":{"pending_delivery_checks":[]}, "selection":{"empty_delivery":false},
        "snapshot":{"mode":"diff","head":"a".repeat(40)}
    });
    for scope in ["delivery", "task"] {
        let mut accepted = report.clone();
        accepted["scope"] = scope.into();
        verify_source_qualitygate(&skill_with_report(temp.path(), &accepted)).unwrap();
    }
    for (pointer, bad) in [
        ("/profile", json!("quick")),
        ("/profile", json!("future")),
        ("/scope", json!("repository")),
        ("/scope", json!("path")),
        ("/scope", json!("future")),
        ("/gate/complete", json!(false)),
        ("/gate/complete", json!("true")),
        ("/gate/decision", json!("fail")),
        ("/gate/decision", json!("incomplete")),
        ("/gate/decision", json!("future")),
        ("/plan/pending_delivery_checks", json!(["tests"])),
        ("/plan/pending_delivery_checks", json!(null)),
        ("/selection/empty_delivery", json!(true)),
        ("/snapshot/mode", json!("worktree")),
        ("/snapshot/mode", json!("staged")),
        ("/snapshot/mode", json!("mr")),
        ("/snapshot/mode", json!("future")),
        ("/snapshot/head", json!("b".repeat(40))),
    ] {
        let mut changed = report.clone();
        *changed.pointer_mut(pointer).unwrap() = bad;
        let skill = skill_with_report(temp.path(), &changed);
        let error = verify_source_qualitygate(&skill).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("complete, nonempty full diff gate"),
            "{pointer}: {error}"
        );
    }
    for pointer in [
        "/profile",
        "/scope",
        "/gate/complete",
        "/gate/decision",
        "/plan/pending_delivery_checks",
        "/selection/empty_delivery",
        "/snapshot/mode",
        "/snapshot/head",
    ] {
        let mut changed = report.clone();
        *changed.pointer_mut(pointer).unwrap() = Value::Null;
        assert!(
            verify_source_qualitygate(&skill_with_report(temp.path(), &changed)).is_err(),
            "{pointer}"
        );
    }
    let skill = skill_with_report(temp.path(), &report);
    fs::write(&skill.source_qualitygate.as_ref().unwrap().path, b"{}").unwrap();
    assert!(
        verify_source_qualitygate(&skill)
            .unwrap_err()
            .to_string()
            .contains("report changed")
    );
}

use super::*;
use freshness::{RemoteClient, RemoteSnapshot};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

pub(super) fn upstream() -> RemoteSnapshot {
    RemoteSnapshot {
        repository: "coolplayagent/example".into(),
        default_branch: "main".into(),
        commit: "0".repeat(40),
        checked_at: "2026-10-09T00:00:00Z".into(),
    }
}

#[cfg(unix)]
pub(super) fn mock_remote(root: &Path) -> RemoteClient {
    use std::os::unix::fs::PermissionsExt;
    let head = root.join("current-upstream.json");
    fs::write(
        &head,
        serde_json::to_vec(&json!({"sha":upstream().commit})).unwrap(),
    )
    .unwrap();
    let gh = root.join("fake-gh");
    let quoted = head.to_string_lossy().replace('\'', "'\\''");
    let release = root.join("current-release.json");
    if !release.exists() {
        fs::write(
            &release,
            r#"{"id":1,"tag_name":"v1","draft":false,"prerelease":false}"#,
        )
        .unwrap();
    }
    let release = release.to_string_lossy().replace('\'', "'\\''");
    fs::write(&gh, format!("#!/bin/sh\nset -eu\ncase \"$1\" in\nrepo) printf '%s\\n' '{{\"nameWithOwner\":\"coolplayagent/example\",\"defaultBranchRef\":{{\"name\":\"main\"}}}}';;\napi) case \"$2\" in */releases/latest) cat '{release}';; *) cat '{quoted}';; esac;;\n*) exit 92;;\nesac\n")).unwrap();
    fs::set_permissions(&gh, fs::Permissions::from_mode(0o755)).unwrap();
    RemoteClient {
        git: PathBuf::from("git"),
        gh,
        timeout: Duration::from_secs(2),
    }
}

pub(super) fn artifact(root: &Path, name: &str, value: Value) -> VerificationArtifact {
    let path = root.join(name);
    fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
    VerificationArtifact {
        sha256: file_sha256(&path).unwrap(),
        path,
    }
}

pub(super) fn evidence(root: &Path) -> ReleaseEvidence {
    let installed = root.join("verified-skill");
    fs::create_dir_all(&installed).unwrap();
    fs::write(
        installed.join("SKILL.md"),
        "---\nname: verified-skill\n---\nFixture",
    )
    .unwrap();
    fs::write(installed.join("cli"), "verified runtime").unwrap();
    let skills_manifest = artifact(
        root,
        "skills-manifest.json",
        json!({"schema_version":1,"skills":[{"name":"verified-skill","repository":"coolplayagent/example","installed_path":installed,"runtime_path":installed.join("cli"),"tree_sha256":freshness::skill_tree_digest(&installed).unwrap(),"runtime_sha256":file_sha256(&installed.join("cli")).unwrap(),"release_tag":"v1","release_id":1,"source_commit":null,"checked_at":"2026-10-09T00:00:00Z"}]}),
    );
    let qualitygate_report = artifact(
        root,
        "qualitygate.json",
        json!({ "profile": "full", "scope": "delivery", "gate": {"complete":true,"decision":"pass"}, "plan":{"pending_delivery_checks":[]},"snapshot":{"content_digest":"e".repeat(64),"verification_digest":"f".repeat(64)} }),
    );
    let snapshot_receipt = artifact(
        root,
        "snapshot.json",
        json!({"source_commit":"a".repeat(40),"upstream":upstream(),"skills_manifest_sha256":skills_manifest.sha256,"prompt_sha256":"b".repeat(64),"policy_sha256":"c".repeat(64),"superpod_commit":"d".repeat(40),"qualitygate_report_sha256":qualitygate_report.sha256,"content_digest":"e".repeat(64),"verification_digest":"f".repeat(64)}),
    );
    let evaluation_receipt = artifact(
        root,
        "evaluation.json",
        json!({"role":"independent_evaluator","status":"pass","complete":true,"source_commit":"a".repeat(40),"upstream":upstream(),"skills_manifest_sha256":skills_manifest.sha256,"prompt_sha256":"b".repeat(64),"policy_sha256":"c".repeat(64),"superpod_commit":"d".repeat(40),"evaluator_id":"evaluation","implementer_id":"implementation"}),
    );
    ReleaseEvidence {
        source_commit: "a".repeat(40),
        upstream: Some(upstream()),
        skills_manifest: Some(skills_manifest),
        prompt_sha256: "b".repeat(64),
        policy_sha256: "c".repeat(64),
        superpod_commit: "d".repeat(40),
        implementer_id: "implementation".into(),
        evaluator_id: "evaluation".into(),
        evaluation_passed: true,
        qualitygate_passed: true,
        qualitygate_report,
        snapshot_receipt,
        evaluation_receipt,
    }
}

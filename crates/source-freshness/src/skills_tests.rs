use crate::{
    SkillSnapshot, SkillsManifest, SourceQualitygate,
    files::hex,
    skill_tree_digest,
    source_gate::verify_source_qualitygate,
    test_support::{fixture, git},
    verify_installed_bytes,
};
use chrono::Utc;
use sha2::{Digest, Sha256};
use std::fs;

#[test]
fn skill_hash_and_upstream_release_or_source_must_all_be_current() {
    let (temp, client, _checkout, author) = fixture();
    let installed = temp.path().join("skill");
    fs::create_dir(&installed).unwrap();
    fs::write(installed.join("SKILL.md"), "Use the tested CLI.").unwrap();
    let runtime = installed.join("runtime");
    fs::write(&runtime, b"runtime-v1").unwrap();
    let mut manifest = SkillsManifest {
        schema_version: 1,
        skills: vec![SkillSnapshot {
            name: "workflow-cli".into(),
            repository: "coolplayagent/freshness-fixture".into(),
            installed_path: installed.clone(),
            runtime_path: runtime.clone(),
            tree_sha256: skill_tree_digest(&installed).unwrap(),
            runtime_sha256: hex(&Sha256::digest(b"runtime-v1")),
            release_tag: Some("v1".into()),
            release_id: Some(42),
            source_commit: None,
            source_qualitygate: None,
            checked_at: Utc::now().to_rfc3339(),
        }],
    };
    let path = temp.path().join("skills.json");
    storage::write(&path, &manifest).unwrap();
    client.verify_skills(&path).unwrap();
    verify_installed_bytes(&manifest).unwrap();
    fs::write(installed.join("SKILL.md"), "modified instructions").unwrap();
    assert!(client.verify_skills(&path).is_err());
    assert!(
        verify_installed_bytes(&manifest).is_err(),
        "a mid-attempt installation must invalidate old observations without a network call"
    );
    fs::write(installed.join("SKILL.md"), "Use the tested CLI.").unwrap();
    fs::write(
        temp.path().join("release.json"),
        r#"{"id":43,"tag_name":"v2","draft":false,"prerelease":false}"#,
    )
    .unwrap();
    assert!(client.verify_skills(&path).is_err());
    manifest.skills[0].release_tag = None;
    manifest.skills[0].release_id = None;
    manifest.skills[0].source_commit = Some(git(&author, &["rev-parse", "HEAD"]));
    storage::write(&path, &manifest).unwrap();
    assert!(
        client.verify_skills(&path).is_err(),
        "an unverified source fallback cannot ignore an existing release"
    );
    let proof_path = temp.path().join("source-gate.json");
    let mut proof = serde_json::json!({
        "profile":"full","scope":"delivery",
        "gate":{"complete":true,"decision":"pass"},
        "plan":{"pending_delivery_checks":[]},
        "selection":{"empty_delivery":false},
        "snapshot":{"mode":"diff","head":manifest.skills[0].source_commit}
    });
    storage::write(&proof_path, &proof).unwrap();
    manifest.skills[0].source_qualitygate = Some(SourceQualitygate {
        path: proof_path.clone(),
        sha256: storage::digest(&fs::read(&proof_path).unwrap()),
    });
    storage::write(&path, &manifest).unwrap();
    client.verify_skills(&path).unwrap();
    let retained_proof = fs::read(&proof_path).unwrap();
    fs::OpenOptions::new()
        .write(true)
        .open(&proof_path)
        .unwrap()
        .set_len(16 * 1024 * 1024 + 1)
        .unwrap();
    assert!(
        verify_source_qualitygate(&manifest.skills[0])
            .unwrap_err()
            .to_string()
            .contains("16 MiB"),
        "oversized reports must be rejected before parsing"
    );
    fs::write(&proof_path, &retained_proof).unwrap();
    let replacement = temp.path().join("replacement-source-gate.json");
    fs::rename(&proof_path, &replacement).unwrap();
    std::os::unix::fs::symlink(&replacement, &proof_path).unwrap();
    assert!(verify_source_qualitygate(&manifest.skills[0]).is_err());
    fs::remove_file(&proof_path).unwrap();
    fs::rename(&replacement, &proof_path).unwrap();
    verify_source_qualitygate(&manifest.skills[0]).unwrap();
    proof["snapshot"]["head"] = "0".repeat(40).into();
    storage::write(&proof_path, &proof).unwrap();
    assert!(
        client.verify_skills(&path).is_err(),
        "changed proof must fail"
    );
    manifest.skills[0]
        .source_qualitygate
        .as_mut()
        .unwrap()
        .sha256 = storage::digest(&fs::read(&proof_path).unwrap());
    storage::write(&path, &manifest).unwrap();
    assert!(
        client.verify_skills(&path).is_err(),
        "a passing gate on another source is insufficient"
    );
    manifest.skills[0].source_qualitygate = None;
    storage::write(&path, &manifest).unwrap();
    fs::write(temp.path().join("no-release"), "").unwrap();
    client.verify_skills(&path).unwrap();
    fs::write(author.join("tracked.txt"), "new source skill version").unwrap();
    git(&author, &["commit", "-am", "source advances"]);
    git(&author, &["push", "origin", "main"]);
    assert!(client.verify_skills(&path).is_err());
}

#[test]
fn invalid_manifest_cannot_return_partial_verified_skills_or_rewrite_input() {
    let (temp, client, _checkout, _author) = fixture();
    let installed = temp.path().join("skill");
    fs::create_dir(&installed).unwrap();
    fs::write(installed.join("SKILL.md"), "Fixture").unwrap();
    let runtime = installed.join("runtime");
    fs::write(&runtime, b"runtime").unwrap();
    let valid = SkillSnapshot {
        name: "valid-skill".into(),
        repository: "coolplayagent/freshness-fixture".into(),
        installed_path: installed.clone(),
        runtime_path: runtime,
        tree_sha256: skill_tree_digest(&installed).unwrap(),
        runtime_sha256: storage::digest(b"runtime"),
        release_tag: Some("v1".into()),
        release_id: Some(42),
        source_commit: None,
        source_qualitygate: None,
        checked_at: "2026-10-09T00:00:00Z".into(),
    };
    let path = temp.path().join("skills.json");
    for change in ["duplicate", "mixed-binding", "bad-digest"] {
        let mut bad = valid.clone();
        if change != "duplicate" {
            bad.name = "second-skill".into();
        }
        match change {
            "mixed-binding" => bad.source_commit = Some("a".repeat(40)),
            "bad-digest" => bad.runtime_sha256 = "0".repeat(64),
            _ => {}
        }
        storage::write(
            &path,
            &SkillsManifest {
                schema_version: 1,
                skills: vec![valid.clone(), bad],
            },
        )
        .unwrap();
        let before = fs::read(&path).unwrap();
        assert!(client.verify_skills(&path).is_err());
        assert_eq!(before, fs::read(&path).unwrap());
    }
    storage::write(
        &path,
        &SkillsManifest {
            schema_version: 1,
            skills: vec![valid],
        },
    )
    .unwrap();
    let before = fs::read(&path).unwrap();
    let verified = client.verify_skills(&path).unwrap();
    assert_eq!(verified.skills.len(), 1);
    assert_ne!(verified.skills[0].checked_at, "2026-10-09T00:00:00Z");
    assert_eq!(before, fs::read(&path).unwrap());
}

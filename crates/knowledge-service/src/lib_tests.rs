use super::*;
use collaboration_model::{EvidencePins, KnowledgeWriteInput, Outcome, OutcomeInput};
use std::process::Command;

fn fixture(git: bool) -> (tempfile::TempDir, Service, Target) {
    let root = tempfile::tempdir().unwrap();
    let target_path = root.path().join("external");
    fs::create_dir_all(target_path.join("knowledge")).unwrap();
    fs::write(target_path.join("knowledge/note.md"), "  source\n").unwrap();
    if git {
        for args in [
            vec!["init", "-b", "main"],
            vec!["config", "user.name", "Test"],
            vec!["config", "user.email", "test@example.org"],
            vec!["add", "."],
            vec!["commit", "-m", "baseline"],
            vec!["branch", "outcomes"],
        ] {
            assert!(
                Command::new("git")
                    .args(["-C", target_path.to_str().unwrap()])
                    .args(args)
                    .output()
                    .unwrap()
                    .status
                    .success()
            );
        }
    }
    let target = Target {
        kind: if git {
            KnowledgeBackend::Git
        } else {
            KnowledgeBackend::Directory
        },
        path: target_path,
        content_root: "knowledge".into(),
        enabled: true,
        access: KnowledgeAccess::ReadWrite,
        reference: git.then(|| "refs/heads/outcomes".into()),
        remote: None,
        credential_env: vec![],
    };
    let service = Service::open("knowledge", &root.path().join("state")).unwrap();
    service
        .manage(Request::Save {
            configuration: Configuration {
                revision: 0,
                targets: std::collections::BTreeMap::from([("external".into(), target.clone())]),
            },
        })
        .unwrap();
    (root, service, target)
}
fn outcome() -> Outcome {
    let input = OutcomeInput {
        id: "finding".into(),
        topic_id: "topic".into(),
        goal_id: "goal".into(),
        kind: OutcomeKind::Finding,
        title: "Validated finding".into(),
        summary: "Evidence summary".into(),
        conditions: "Pinned test".into(),
        evidence: vec![],
        references: vec![],
        supersedes: None,
        attempt: None,
        pins: EvidencePins::default(),
        knowledge: None,
    };
    Outcome {
        digest: storage::digest(&serde_json::to_vec(&("operator", &input)).unwrap()),
        input,
        author_id: "operator".into(),
        created_ms: 1,
        sequence: 1,
    }
}
fn preview(service: &Service) -> Preview {
    serde_json::from_value(
        service
            .manage(Request::Preview {
                target: "external".into(),
                outcome: Box::new(outcome()),
            })
            .unwrap(),
    )
    .unwrap()
}
fn write(preview: &Preview) -> KnowledgeWrite {
    let outcome = outcome();
    let input = KnowledgeWriteInput {
        id: "publication".into(),
        outcome_id: outcome.input.id.clone(),
        outcome_digest: outcome.digest,
        acceptance_id: "acceptance".into(),
        service_id: "knowledge".into(),
        service_digest: "a".repeat(64),
        repository: "external".into(),
        expected_revision: preview.expected_revision.clone(),
        max_seconds: 60,
    };
    KnowledgeWrite {
        request_digest: storage::digest(&serde_json::to_vec(&input).unwrap()),
        input,
        created_ms: collaboration_model::now_ms(),
        deadline_ms: collaboration_model::now_ms() + 60000,
        result: None,
    }
}
fn publish(service: &Service, write: &KnowledgeWrite) -> Result<Value> {
    service.dispatch(OutcomeServiceRequest::Write {
        write: write.clone(),
        outcome: Box::new(outcome()),
    })
}
#[test]
fn adapters_share_preview_publish_read_and_idempotent_receipt_contract() {
    for git in [false, true] {
        let (_root, service, target) = fixture(git);
        let read = service
            .manage(Request::Read {
                target: "external".into(),
                path: "note.md".into(),
                revision: None,
            })
            .unwrap();
        assert_eq!(read["content"], "  source\n");
        let preview = preview(&service);
        assert!(preview.before.is_none());
        assert!(preview.expected_revision.contains('@'));
        let write = write(&preview);
        let result = publish(&service, &write).unwrap();
        assert_eq!(result["state"], KnowledgeWriteState::Applied.as_str());
        assert_eq!(publish(&service, &write).unwrap(), result);
        let receipt = service
            .dispatch(OutcomeServiceRequest::LookupWrite {
                id: write.input.id.clone(),
                request_digest: write.request_digest.clone(),
            })
            .unwrap();
        assert_eq!(receipt, result);
        let read = service
            .manage(Request::Read {
                target: "external".into(),
                path: "accepted-outcomes/finding.json".into(),
                revision: None,
            })
            .unwrap();
        let restored: Outcome = serde_json::from_str(read["content"].as_str().unwrap()).unwrap();
        assert_eq!(restored.digest, outcome().digest);
        if git {
            assert!(!target.path.join("knowledge/accepted-outcomes").exists());
        }
        let mut conflict = write.clone();
        conflict.input.acceptance_id = "other".into();
        conflict.request_digest = storage::digest(&serde_json::to_vec(&conflict.input).unwrap());
        assert!(publish(&service, &conflict).is_err());
    }
}
#[test]
fn configuration_changes_invalidate_preview_and_readonly_is_enforced() {
    for git in [false, true] {
        let (_root, service, _) = fixture(git);
        let write = write(&preview(&service));
        let mut config = service.configuration().unwrap();
        config.targets.get_mut("external").unwrap().access = KnowledgeAccess::ReadOnly;
        service
            .manage(Request::Save {
                configuration: config.clone(),
            })
            .unwrap();
        assert!(
            service
                .manage(Request::Save {
                    configuration: config
                })
                .is_err()
        );
        assert!(publish(&service, &write).is_err());
        let mut config = service.configuration().unwrap();
        config.targets.get_mut("external").unwrap().access = KnowledgeAccess::ReadWrite;
        service
            .manage(Request::Save {
                configuration: config,
            })
            .unwrap();
        assert!(
            publish(&service, &write)
                .unwrap_err()
                .to_string()
                .contains("configuration_conflict")
        );
        assert!(
            service
                .manage(Request::Read {
                    target: "external".into(),
                    path: "../escape".into(),
                    revision: None
                })
                .is_err()
        );
    }
}
#[test]
fn directory_conflict_symlink_and_readonly_reconciliation_never_claim_success() {
    let (_root, service, target) = fixture(false);
    let write = write(&preview(&service));
    fs::write(target.path.join("knowledge/note.md"), "concurrent edit").unwrap();
    assert_ne!(
        publish(&service, &write).unwrap()["state"],
        KnowledgeWriteState::Applied.as_str()
    );
    assert!(
        !target
            .path
            .join("knowledge/accepted-outcomes/finding.json")
            .exists()
    );
    let before = fs::read(target.path.join("knowledge/note.md")).unwrap();
    let receipt = service
        .dispatch(OutcomeServiceRequest::LookupWrite {
            id: write.input.id.clone(),
            request_digest: write.request_digest.clone(),
        })
        .unwrap();
    assert_ne!(receipt["state"], KnowledgeWriteState::Applied.as_str());
    assert_eq!(
        fs::read(target.path.join("knowledge/note.md")).unwrap(),
        before
    );
    std::os::unix::fs::symlink("/etc", target.path.join("knowledge/escape")).unwrap();
    assert!(
        service
            .manage(Request::Probe {
                target: "external".into()
            })
            .is_err()
    );
    assert!(
        service
            .manage(Request::Read {
                target: "external".into(),
                path: "escape/passwd".into(),
                revision: None
            })
            .is_err()
    );
}
#[test]
fn target_configuration_never_accepts_embedded_store_or_inline_credentials() {
    let (_root, service, mut target) = fixture(false);
    target.path = service.state.clone();
    assert!(
        service
            .manage(Request::Save {
                configuration: Configuration {
                    revision: 1,
                    targets: std::collections::BTreeMap::from([("external".into(), target)])
                }
            })
            .is_err()
    );
    let raw = json!({"kind":"git","path":"/repo","content_root":"knowledge","enabled":true,"access":"read_write","reference":"refs/heads/outcomes","remote":"origin","credential_env":[],"token":"secret"});
    assert!(serde_json::from_value::<Target>(raw).is_err());
}
#[tokio::test]
async fn management_rpc_matches_direct_contract_and_is_versioned() {
    let (_root, service, _) = fixture(false);
    let socket = service.state.join("knowledge.sock");
    let state = service.state.clone();
    let socket_task = socket.clone();
    let task = tokio::spawn(async move { serve("knowledge", &state, &socket_task, 120).await });
    for _ in 0..100 {
        if socket.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let response: Value = service_protocol::call(
        &socket,
        &OutcomeServiceRequest::Knowledge {
            request: Request::List,
        },
        Duration::from_secs(5),
    )
    .await
    .unwrap();
    assert_eq!(response, service.manage(Request::List).unwrap());
    task.abort();
    let _ = task.await;
}

#[test]
fn preview_is_bound_to_outcome_version_and_never_authorizes_replacement_content() {
    let (_root, service, _) = fixture(false);
    let mut write = write(&preview(&service));
    let mut replacement = outcome();
    replacement.input.summary = "Changed after preview".into();
    replacement.digest = storage::digest(
        &serde_json::to_vec(&(&replacement.author_id, &replacement.input)).unwrap(),
    );
    write.input.outcome_digest = replacement.digest.clone();
    write.request_digest = storage::digest(&serde_json::to_vec(&write.input).unwrap());
    assert!(
        service
            .dispatch(OutcomeServiceRequest::Write {
                write,
                outcome: Box::new(replacement)
            })
            .unwrap_err()
            .to_string()
            .contains("configuration_conflict")
    );
}

#[test]
fn local_directory_add_is_atomic_idempotent_and_supports_the_selected_root() {
    let (root, service, mut target) = fixture(false);
    target.content_root = ".".into();
    target.access = KnowledgeAccess::ReadOnly;
    let request = || Request::Add {
        name: "whole".into(),
        target: target.clone(),
    };
    let added = service.manage(request()).unwrap();
    assert_eq!(
        added["configuration"]["targets"]["whole"]["content_root"],
        "."
    );
    assert_eq!(
        service.manage(request()).unwrap()["configuration"]["revision"],
        added["configuration"]["revision"]
    );
    let value = service
        .manage(Request::Read {
            target: "whole".into(),
            path: "knowledge/note.md".into(),
            revision: None,
        })
        .unwrap();
    assert_eq!(value["content"], "  source\n");
    assert!(!target.path.join(".outcomehub").exists());
    let mut conflicting = target.clone();
    conflicting.access = KnowledgeAccess::ReadWrite;
    assert!(
        service
            .manage(Request::Add {
                name: "whole".into(),
                target: conflicting
            })
            .is_err()
    );
    let mut invalid = target.clone();
    invalid.path = root.path().join("missing");
    assert!(
        service
            .manage(Request::Add {
                name: "missing".into(),
                target: invalid
            })
            .is_err()
    );
    assert_eq!(
        service.manage(Request::List).unwrap()["revision"],
        added["configuration"]["revision"]
    );
    assert!(knowledge_model::relative(Path::new(".")).is_err());
    assert!(
        service
            .manage(Request::Read {
                target: "whole".into(),
                path: "../outside".into(),
                revision: None
            })
            .is_err()
    );
}

#[test]
fn local_git_detects_read_branch_without_writes_and_requires_prepared_write_branch() {
    let (_root, service, mut target) = fixture(true);
    target.content_root = ".".into();
    target.access = KnowledgeAccess::ReadOnly;
    target.reference = None;
    let added = service
        .manage(Request::Add {
            name: "whole".into(),
            target: target.clone(),
        })
        .unwrap();
    assert_eq!(added["target"]["reference"], "refs/heads/main");
    let value = service
        .manage(Request::Read {
            target: "whole".into(),
            path: "knowledge/note.md".into(),
            revision: None,
        })
        .unwrap();
    assert_eq!(value["content"], "  source\n");
    let status = Command::new("git")
        .arg("-C")
        .arg(&target.path)
        .args(["status", "--porcelain"])
        .output()
        .unwrap();
    assert!(status.status.success() && status.stdout.is_empty());
    target.access = KnowledgeAccess::ReadWrite;
    assert!(
        service
            .manage(Request::Inspect {
                target: target.clone()
            })
            .is_err()
    );
    target.reference = Some("refs/heads/main".into());
    assert!(
        service
            .manage(Request::Inspect {
                target: target.clone()
            })
            .is_err()
    );
    target.reference = Some("refs/heads/outcomes".into());
    service
        .manage(Request::Add {
            name: "publish".into(),
            target: target.clone(),
        })
        .unwrap();
    let preview: Preview = serde_json::from_value(
        service
            .manage(Request::Preview {
                target: "publish".into(),
                outcome: Box::new(outcome()),
            })
            .unwrap(),
    )
    .unwrap();
    assert_eq!(preview.path, "accepted-outcomes/finding.json");
    let mut request = write(&preview);
    request.input.repository = "publish".into();
    request.request_digest = storage::digest(&serde_json::to_vec(&request.input).unwrap());
    let result = service
        .dispatch(OutcomeServiceRequest::Write {
            write: request,
            outcome: Box::new(outcome()),
        })
        .unwrap();
    assert_eq!(result["state"], "applied");
}

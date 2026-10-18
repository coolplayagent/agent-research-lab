use crate::{
    host::relative,
    model::{Policy, Prepared, Repository, Request},
    service::unit,
};
use serde_json::json;
use std::collections::BTreeMap;
use std::path::Path;

pub(super) fn admission(operation: &Path) {
    storage::write(
        &operation.join("admission.json"),
        &collaboration_model::EvolutionChange {
            request_id: "activation-claim".into(),
            run_id: "iteration".into(),
            revision: 7,
            action: collaboration_model::EvolutionAction::BeginActivation {
                claim_id: "activation-claim".into(),
                max_seconds: 3600,
            },
        },
    )
    .unwrap();
}

pub(super) fn prepared(state_dir: &Path) -> Prepared {
    let artifact = delivery::VerificationArtifact {
        path: state_dir.join("fixture-evidence.json"),
        sha256: "e".repeat(64),
    };
    let repository = Repository {
        checkout: state_dir.join("checkout"),
        build_commands: vec![vec!["bazel".into(), "build".into(), "//...".into()]],
        package_directory: "dist/skill".into(),
        binary_relative_path: "assets/linux/tool".into(),
        probe: vec!["--help".into()],
        platform_units: vec![],
        qualitygate_snapshot_max_file_mib: None,
    };
    Prepared {
        request: Request {
            schema_version: 1,
            iteration_id: "iteration".into(),
            goal_id: "goal".into(),
            candidate_commit: "a".repeat(40),
            merged_commit: "b".repeat(40),
            repository: "owner/repo".into(),
            merge: json!({}),
            // Journal/early-failure fixtures never pass this historical evidence to admission.
            evidence: delivery::ReleaseEvidence {
                source_commit: "a".repeat(40),
                upstream: None,
                skills_manifest: None,
                prompt_sha256: "c".repeat(64),
                policy_sha256: "d".repeat(64),
                superpod_commit: "f".repeat(40),
                implementer_id: "implementer".into(),
                evaluator_id: "reviewer".into(),
                evaluation_passed: false,
                qualitygate_passed: false,
                qualitygate_report: artifact.clone(),
                snapshot_receipt: artifact.clone(),
                evaluation_receipt: artifact,
            },
        },
        policy: Policy {
            schema_version: 1,
            service_config: state_dir.join("absent-config.toml"),
            main_unit: "fixture.service".into(),
            health_url: "http://127.0.0.1:1/".into(),
            repositories: BTreeMap::from([("owner/repo".into(), repository.clone())]),
            storage_state_dir: None,
            storage_socket: Some(state_dir.join("absent-storage.sock")),
        },
        repository,
        state_dir: state_dir.to_owned(),
        config_sha256: "1".repeat(64),
        policy_sha256: "2".repeat(64),
    }
}

#[test]
fn artifact_paths_and_service_units_cannot_escape_host_configuration() {
    for value in ["../binary", "/bin/tool", "assets/../tool", ""] {
        assert!(relative(Path::new(value)).is_err());
    }
    relative(Path::new("assets/linux/tool")).unwrap();
    for value in ["../service", "service\nExecStart=evil", "service;evil", ""] {
        assert!(unit(value).is_err());
    }
    unit("agent-research-lab.service").unwrap();
}

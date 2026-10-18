//! Local deterministic fixtures; no production authority or external publication.
use crate::dispatch::execute_with_client;
use anyhow::Result;
use freshness::RemoteClient;
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};

pub(super) fn mock_remote(root: &Path) -> RemoteClient {
    use std::os::unix::fs::PermissionsExt;
    let head = root.join("current-upstream.json");
    if !head.exists() {
        storage::write(&head, &json!({"sha":"a".repeat(40)})).unwrap();
    }
    let gh = root.join("fake-gh");
    let quoted = head.to_string_lossy().replace('\'', "'\\''");
    let release = root.join("current-release.json");
    if !release.exists() {
        storage::write(
            &release,
            &json!({"id":1,"tag_name":"v1","draft":false,"prerelease":false}),
        )
        .unwrap();
    }
    let release = release.to_string_lossy().replace('\'', "'\\''");
    fs::write(&gh, format!("#!/bin/sh\nset -eu\ncase \"$1\" in\nrepo) printf '%s\\n' '{{\"nameWithOwner\":\"coolplayagent/example\",\"defaultBranchRef\":{{\"name\":\"main\"}}}}';;\napi) case \"$2\" in */releases/latest) cat '{release}';; *) cat '{quoted}';; esac;;\n*) exit 92;;\nesac\n")).unwrap();
    fs::set_permissions(&gh, fs::Permissions::from_mode(0o755)).unwrap();
    RemoteClient {
        git: PathBuf::from("git"),
        gh,
        timeout: std::time::Duration::from_secs(2),
    }
}

pub(super) fn execute_fixture(action: &str, input: &Path, output: Option<&Path>) -> Result<Value> {
    let host = input.parent().unwrap();
    execute_with_client(action, input, output, &mock_remote(host))
}

pub(super) fn example(name: &str) -> Value {
    serde_json::from_str(match name {
        "prompt-register" => include_str!("../../../examples/evolution/prompt-register.json"),
        "context" => include_str!("../../../examples/evolution/context.json"),
        "evaluate" => include_str!("../../../examples/evolution/evaluate.json"),
        "novelty" => include_str!("../../../examples/evolution/novelty.json"),
        "exploration" => include_str!("../../../examples/evolution/exploration.json"),
        "rule-proposal" => include_str!("../../../examples/evolution/rule-proposal.json"),
        "promote" => include_str!("../../../examples/evolution/promote.json"),
        _ => panic!("unknown fixture"),
    })
    .unwrap()
}

pub(super) fn setup() -> tempfile::TempDir {
    let temp = tempfile::tempdir().unwrap();
    for name in ["host", "holdout", "optimizer", "superpod", "memory"] {
        fs::create_dir(temp.path().join(name)).unwrap();
    }
    temp
}

pub(super) fn prepare_gate(temp: &Path, action: &str) -> PathBuf {
    let mut request = example(action);
    request["context_file"] = "context.json".into();
    request["holdout_isolation"] = json!({
        "private_root":"../holdout","optimizer_roots":["../optimizer"],
        "shared_knowledge_roots":["../superpod"],"shared_memory_roots":["../memory"]
    });
    let installed = temp.join("host/verified-skill");
    fs::create_dir_all(&installed).unwrap();
    fs::write(
        installed.join("SKILL.md"),
        "---\nname: verified-skill\n---\nFixture",
    )
    .unwrap();
    fs::write(installed.join("cli"), "verified runtime").unwrap();
    let manifest = temp.join("host/skills-manifest.json");
    storage::write(&manifest, &json!({"schema_version":1,"skills":[{"name":"verified-skill","repository":"coolplayagent/example","installed_path":installed,"runtime_path":installed.join("cli"),"tree_sha256":freshness::skill_tree_digest(&installed).unwrap(),"runtime_sha256":delivery::file_sha256(&installed.join("cli")).unwrap(),"release_tag":"v1","release_id":1,"source_commit":null,"checked_at":"2026-10-09T00:00:00Z"}]})).unwrap();
    request["skills_manifest"] =
        json!({"path":"skills-manifest.json","sha256":delivery::file_sha256(&manifest).unwrap()});
    storage::write(&temp.join("host/context.json"), &example("context")).unwrap();
    let path = temp.join(format!("host/{action}.json"));
    storage::write(&path, &request).unwrap();
    path
}

use super::*;
use crate::job_store::job_path;
use crate::scheduler::run_scoped;
use crate::tests::completion_fixture;
use native_effects::fs;
use serde_json::json;
use std::collections::BTreeSet;

#[test]
fn task_scope_rejects_invalid_files_missing_dependencies_and_configuration_drift() {
    let (temp, mut c, mut job, _) = completion_fixture(false, json!([]));
    for name in [
        "workflow-cli",
        "relay-knowledge",
        "into-markdown",
        "qualitygate-cli",
        "computer-use-cli",
        "repo-sandbox",
    ] {
        c.tools.insert(name.into(), c.tools["relay-memory"].clone());
    }
    job.config_digest = storage::digest(&serde_json::to_vec(&c).unwrap());
    storage::write(&job_path(&c, &job.task.id), &job).unwrap();
    let path = c.state_dir.join("private/scope.json");
    storage::write(&path, &vec![job.task.id.clone()]).unwrap();
    let scope = TaskScope::load(&c, &path).unwrap();
    assert_eq!(scope.ids, BTreeSet::from([job.task.id.clone()]));
    for (continuous, seconds, seed) in [(true, 30, false), (false, 30, true), (false, 0, false)] {
        assert!(
            run_scoped(&c, continuous, seconds, seed, Some(&path))
                .unwrap_err()
                .to_string()
                .contains("scoped runs")
        );
    }
    for ids in [
        vec![],
        vec![job.task.id.clone(), job.task.id.clone()],
        vec!["missing".into()],
        vec!["../escape".into()],
        vec!["x".into(); 257],
    ] {
        storage::write(&path, &ids).unwrap();
        assert!(TaskScope::load(&c, &path).is_err());
    }
    storage::write(&path, &vec![job.task.id.clone()]).unwrap();
    let mut drifted = c.clone();
    drifted.task_timeout_seconds += 1;
    assert!(
        TaskScope::load(&drifted, &path)
            .err()
            .unwrap()
            .to_string()
            .contains("configuration")
    );
    job.task.dependencies = vec!["excluded".into()];
    storage::write(&job_path(&c, &job.task.id), &job).unwrap();
    assert!(TaskScope::load(&c, &path).is_err());
    let outside = temp.path().join("outside.json");
    storage::write(&outside, &vec![job.task.id.clone()]).unwrap();
    assert!(TaskScope::load(&c, &outside).is_err());
    let linked = c.state_dir.join("private/linked.json");
    std::os::unix::fs::symlink(&outside, &linked).unwrap();
    assert!(TaskScope::load(&c, &linked).is_err());
    fs::write(&path, vec![b' '; 64 * 1024 + 1]).unwrap();
    assert!(TaskScope::load(&c, &path).is_err());
}

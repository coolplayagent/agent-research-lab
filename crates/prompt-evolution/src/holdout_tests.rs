use super::*;

#[test]
fn holdouts_cannot_overlap_optimizer_or_shared_knowledge() {
    let temp = tempfile::tempdir().unwrap();
    let private = temp.path().join("holdout");
    let optimizer = temp.path().join("optimizer");
    let knowledge = temp.path().join("superpod");
    for path in [&private, &optimizer, &knowledge] {
        std::fs::create_dir(path).unwrap();
    }
    let mut policy = HoldoutIsolation {
        private_root: private.clone(),
        optimizer_roots: vec![optimizer.clone()],
        shared_knowledge_roots: vec![knowledge],
        shared_memory_roots: vec![],
    };
    policy.validate().unwrap();
    policy.authorize_optimizer_path(&optimizer).unwrap();
    assert!(policy.authorize_optimizer_path(&private).is_err());
    policy
        .shared_knowledge_roots
        .push(temp.path().to_path_buf());
    assert!(policy.validate().is_err());
}

#[cfg(unix)]
#[test]
fn nested_symlinks_cannot_expose_holdout_answers() {
    let temp = tempfile::tempdir().unwrap();
    let private = temp.path().join("holdout");
    let optimizer = temp.path().join("optimizer");
    std::fs::create_dir(&private).unwrap();
    std::fs::create_dir(&optimizer).unwrap();
    std::os::unix::fs::symlink(&private, optimizer.join("answers")).unwrap();
    let policy = HoldoutIsolation {
        private_root: private,
        optimizer_roots: vec![optimizer],
        shared_knowledge_roots: vec![],
        shared_memory_roots: vec![],
    };
    assert!(policy.validate().is_err());
}

#[cfg(unix)]
#[test]
fn in_workspace_symlink_cycles_terminate_and_external_directories_are_rejected() {
    let temp = tempfile::tempdir().unwrap();
    let private = temp.path().join("holdout");
    let optimizer = temp.path().join("optimizer");
    let outside = temp.path().join("outside");
    for directory in [&private, &optimizer, &outside] {
        std::fs::create_dir(directory).unwrap();
    }
    std::os::unix::fs::symlink(&optimizer, optimizer.join("loop")).unwrap();
    let policy = HoldoutIsolation {
        private_root: private,
        optimizer_roots: vec![optimizer.clone()],
        shared_knowledge_roots: vec![],
        shared_memory_roots: vec![],
    };
    policy.validate().unwrap();
    std::os::unix::fs::symlink(&outside, optimizer.join("external")).unwrap();
    assert!(
        policy
            .validate()
            .unwrap_err()
            .to_string()
            .contains("escapes declared workspace")
    );
}

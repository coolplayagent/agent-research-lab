use super::*;

#[test]
fn soul_reads_frozen_blob_and_never_falls_back_to_mutable_or_missing_custom_prompt() {
    let root = std::env::temp_dir().join(format!(
        "lab-persona-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(root.clone());
    let repo = root.join("agent-research-lab");
    fs::create_dir_all(repo.join("prompts")).unwrap();
    fs::write(repo.join("prompts/research.md"), "FROZEN_ROLE").unwrap();
    git(&repo, &["init", "-q"]).unwrap();
    git(&repo, &["add", "prompts/research.md"]).unwrap();
    git(
        &repo,
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@localhost",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-qm",
            "role",
        ],
    )
    .unwrap();
    let commit = git(&repo, &["rev-parse", "HEAD"]).unwrap();
    fs::write(repo.join("prompts/research.md"), "MUTABLE_PRIVATE_CONTENT").unwrap();
    let mut job = json!({"role":"research","run_id":"a","repositories":{"agent-research-lab":commit.trim()},"profile":{}});
    let view = read(&root, &root, &job);
    assert_eq!(view["soul"]["content"], "FROZEN_ROLE");
    assert_eq!(view["soul"]["sha256"], storage::digest(b"FROZEN_ROLE"));
    assert!(!view.to_string().contains("PRIVATE"));
    job["profile"]["prompt_version"] = json!("missing-custom-version");
    assert_eq!(read(&root, &root, &job)["soul"]["available"], false);
    job["profile"] = json!({});
    job["repositories"] = json!({});
    assert_eq!(read(&root, &root, &job)["soul"]["available"], false);
}

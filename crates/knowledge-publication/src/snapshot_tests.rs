use super::*;
use crate::test_support::publication_fixture;

#[cfg(unix)]
#[test]
fn publication_rejects_extra_committed_files() {
    let (_temp, request) = publication_fixture();
    validate_prepared_publication(&request).unwrap();
    fs::write(
        request.prepared.worktree.join("unreviewed.txt"),
        "must not publish",
    )
    .unwrap();
    git(&request.prepared.worktree, &["add", "unreviewed.txt"]).unwrap();
    git(
        &request.prepared.worktree,
        &["commit", "-m", "unreviewed file"],
    )
    .unwrap();
    assert!(
        validate_prepared_publication(&request)
            .unwrap_err()
            .to_string()
            .contains("unexpected publication file")
    );
}

#[cfg(unix)]
#[test]
fn publication_binds_index_map_and_committed_snapshot() {
    let (_temp, mut request) = publication_fixture();
    let index = request
        .prepared
        .worktree
        .join("knowledge/software/index.md");
    let original = fs::read(&index).unwrap();
    fs::write(&index, "changed after review").unwrap();
    assert!(validate_prepared_publication(&request).is_err());
    fs::write(&index, original).unwrap();
    let map = request
        .prepared
        .worktree
        .join("knowledge/knowledge-map.yaml");
    fs::write(&map, "fixture map").unwrap();
    request.prepared.publication_files = publication_snapshot(&request.prepared, None).unwrap();
    fs::write(&map, "map changed after review").unwrap();
    assert!(validate_prepared_publication(&request).is_err());
    fs::write(&map, "fixture map").unwrap();
    validate_prepared_publication(&request).unwrap();
    git(&request.prepared.worktree, &["add", "."]).unwrap();
    git(
        &request.prepared.worktree,
        &["commit", "-m", "reviewed contribution"],
    )
    .unwrap();
    let head = git(&request.prepared.worktree, &["rev-parse", "HEAD"]).unwrap();
    assert_eq!(
        publication_snapshot(&request.prepared, Some(&head)).unwrap(),
        request.prepared.publication_files
    );
    validate_prepared_publication(&request).unwrap();
}

#[cfg(unix)]
#[test]
fn reviewed_deletions_remain_bound_in_worktree_and_commit_snapshots() {
    let (_temp, mut request) = publication_fixture();
    let relative = PathBuf::from("knowledge/software/index.md");
    fs::remove_file(request.prepared.worktree.join(&relative)).unwrap();
    assert!(validate_prepared_publication(&request).is_err());
    request.prepared.publication_files = publication_snapshot(&request.prepared, None).unwrap();
    assert_eq!(
        request.prepared.publication_files.get(&relative),
        Some(&None)
    );
    validate_prepared_publication(&request).unwrap();
    git(&request.prepared.worktree, &["add", "."]).unwrap();
    git(
        &request.prepared.worktree,
        &["commit", "-m", "reviewed deletion"],
    )
    .unwrap();
    let head = git(&request.prepared.worktree, &["rev-parse", "HEAD"]).unwrap();
    assert_eq!(
        publication_snapshot(&request.prepared, Some(&head)).unwrap(),
        request.prepared.publication_files
    );
}

#[cfg(unix)]
#[test]
fn report_symlinks_cannot_reuse_reviewed_bytes() {
    let (_temp, request) = publication_fixture();
    let file = request
        .prepared
        .worktree
        .join(&request.prepared.report_path);
    let outside = request
        .prepared
        .worktree
        .parent()
        .unwrap()
        .join("outside.md");
    fs::rename(&file, &outside).unwrap();
    std::os::unix::fs::symlink(&outside, &file).unwrap();
    assert!(
        publication_snapshot(&request.prepared, None)
            .unwrap_err()
            .to_string()
            .contains("regular file without symlink aliases")
    );
}

use super::*;
use crate::{inspect, root};
use contracts::{KnowledgeAccess, KnowledgeBackend};
use knowledge_model::Target;
fn target(path: &Path) -> Target {
    Target {
        kind: KnowledgeBackend::Directory,
        path: path.into(),
        content_root: ".".into(),
        enabled: true,
        access: KnowledgeAccess::ReadOnly,
        reference: None,
        remote: None,
        credential_env: vec![],
    }
}
fn code(error: anyhow::Error) -> Code {
    error.downcast_ref::<Error>().unwrap().code
}
#[test]
fn large_attachments_are_hashed_without_becoming_text_responses() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("note.md"), "note").unwrap();
    fs::write(temp.path().join("large.pdf"), vec![b'x'; 2 * 1024 * 1024]).unwrap();
    let target = target(temp.path());
    let first = inspect(&target, None, None).unwrap().0;
    assert!(first.starts_with("directory-v2:"));
    assert_eq!(
        inspect(&target, Some(Path::new("note.md")), Some(&first))
            .unwrap()
            .1
            .as_deref(),
        Some("note")
    );
    assert_eq!(
        code(inspect(&target, Some(Path::new("large.pdf")), None).unwrap_err()),
        Code::DocumentLimit
    );
    fs::write(temp.path().join("large.pdf"), vec![b'y'; 2 * 1024 * 1024]).unwrap();
    assert_ne!(first, inspect(&target, None, None).unwrap().0);
    assert_eq!(
        code(inspect(&target, None, Some(&first)).unwrap_err()),
        Code::RevisionConflict
    );
}
#[test]
fn nested_ignore_rules_and_reserved_metadata_apply_to_reads_and_revisions() {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir_all(temp.path().join(".git/objects")).unwrap();
    fs::create_dir(temp.path().join("tmp")).unwrap();
    fs::create_dir(temp.path().join("docs")).unwrap();
    fs::write(temp.path().join(".gitignore"), "tmp/\n*.bin\n").unwrap();
    fs::write(
        temp.path().join("docs/.gitignore"),
        "!keep.bin\nignored.md\n",
    )
    .unwrap();
    fs::write(temp.path().join("docs/keep.bin"), "included").unwrap();
    fs::write(temp.path().join("docs/drop.bin"), "excluded").unwrap();
    fs::write(temp.path().join("docs/ignored.md"), "excluded").unwrap();
    let target = target(temp.path());
    let first = inspect(&target, None, None).unwrap().0;
    fs::write(
        temp.path().join(".git/objects/pack"),
        vec![0; 2 * 1024 * 1024],
    )
    .unwrap();
    fs::write(temp.path().join("tmp/model.bin"), vec![0; 2 * 1024 * 1024]).unwrap();
    fs::write(temp.path().join("docs/drop.bin"), "modified excluded").unwrap();
    assert_eq!(first, inspect(&target, None, None).unwrap().0);
    assert_eq!(
        inspect(&target, Some(Path::new("docs/keep.bin")), None)
            .unwrap()
            .1
            .as_deref(),
        Some("included")
    );
    for path in [
        ".git/objects/pack",
        "tmp/model.bin",
        "docs/drop.bin",
        "docs/ignored.md",
        ".outcomehub/secret.json",
        "docs/.outcomehub/secret.json",
    ] {
        assert_eq!(
            code(inspect(&target, Some(Path::new(path)), None).unwrap_err()),
            Code::ExcludedPath
        );
    }
    fs::write(temp.path().join("docs/.gitignore"), "!keep.bin\n").unwrap();
    assert_ne!(first, inspect(&target, None, None).unwrap().0);
}
#[test]
fn ignored_publication_destinations_and_parent_directories_are_rejected() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join(".gitignore"), "accepted-outcomes/\n").unwrap();
    let root = root(&target(temp.path())).unwrap();
    assert_eq!(
        code(
            allowed(
                &root,
                Path::new("accepted-outcomes/new.json"),
                &mut Budget::new()
            )
            .unwrap_err()
        ),
        Code::ExcludedPath
    );
}
#[test]
fn entry_byte_deadline_depth_and_special_file_limits_are_explicit() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("note"), "content").unwrap();
    let root = root(&target(temp.path())).unwrap();
    let mut budget = Budget::new();
    budget.max_bytes = 3;
    let error = snapshot(&root, &mut budget).unwrap_err();
    assert_eq!(
        error.downcast_ref::<Error>().unwrap().parameters["limit"],
        "3"
    );
    assert_eq!(code(error), Code::SnapshotLimit);
    let mut budget = Budget::new();
    budget.max_entries = 0;
    assert_eq!(
        code(snapshot(&root, &mut budget).unwrap_err()),
        Code::FileLimit
    );
    let mut budget = Budget::new();
    budget.deadline = Instant::now();
    assert_eq!(
        code(snapshot(&root, &mut budget).unwrap_err()),
        Code::Timeout
    );
    std::os::unix::fs::symlink("/etc/passwd", temp.path().join("link")).unwrap();
    assert_eq!(
        code(snapshot(&root, &mut Budget::new()).unwrap_err()),
        Code::SymlinkRejected
    );
    fs::remove_file(temp.path().join("link")).unwrap();
    let path =
        std::ffi::CString::new(temp.path().join("pipe").as_os_str().as_encoded_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
    assert_eq!(
        code(snapshot(&root, &mut Budget::new()).unwrap_err()),
        Code::SpecialFileRejected
    );
    fs::remove_file(temp.path().join("pipe")).unwrap();
    let nested = (0..34).fold(temp.path().to_path_buf(), |p, _| p.join("level"));
    fs::create_dir_all(nested).unwrap();
    assert_eq!(
        code(snapshot(&root, &mut Budget::new()).unwrap_err()),
        Code::DepthLimit
    );
}
#[test]
fn global_ignore_configuration_is_never_consulted_and_rules_are_versioned() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("note"), "content").unwrap();
    let target = target(temp.path());
    let first = inspect(&target, None, None).unwrap().0;
    fs::write(temp.path().join(".gitignore"), "# policy comment\n").unwrap();
    assert_ne!(first, inspect(&target, None, None).unwrap().0);
    assert_eq!(
        code(inspect(&target, None, Some("legacy-unversioned-digest")).unwrap_err()),
        Code::RevisionConflict
    );
}

#[test]
fn concurrent_file_changes_never_produce_a_successful_stable_revision() {
    use std::os::unix::fs::FileExt;
    use std::sync::{
        Arc, Barrier,
        atomic::{AtomicBool, Ordering},
    };
    let temp = tempfile::tempdir().unwrap();
    let file = temp.path().join("changing.bin");
    fs::write(&file, vec![0; 4 * 1024 * 1024]).unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let writer_stop = stop.clone();
    let barrier = Arc::new(Barrier::new(2));
    let writer_barrier = barrier.clone();
    let writer = std::thread::spawn(move || {
        let file = OpenOptions::new().write(true).open(file).unwrap();
        let mut value = 0u8;
        writer_barrier.wait();
        while !writer_stop.load(Ordering::Acquire) {
            file.write_at(&[value], 0).unwrap();
            value = value.wrapping_add(1);
            std::thread::yield_now();
        }
    });
    barrier.wait();
    let result = inspect(&target(temp.path()), None, None);
    stop.store(true, Ordering::Release);
    writer.join().unwrap();
    assert_eq!(code(result.unwrap_err()), Code::ContentChanged);
}

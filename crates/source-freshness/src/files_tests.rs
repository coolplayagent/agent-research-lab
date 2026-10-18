use super::*;

#[test]
fn tree_digest_matches_public_contract_and_rejects_symlinks() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("b.txt"), b"B").unwrap();
    fs::write(temp.path().join("a.txt"), b"A").unwrap();
    fs::create_dir(temp.path().join("folder")).unwrap();
    fs::write(temp.path().join("folder/a.txt"), b"D").unwrap();
    fs::write(temp.path().join("folder.txt"), b"C").unwrap();
    let mut expected = Sha256::new();
    for (path, content) in [
        ("a.txt", b"A"),
        ("b.txt", b"B"),
        ("folder.txt", b"C"),
        ("folder/a.txt", b"D"),
    ] {
        expected.update(path.as_bytes());
        expected.update([0]);
        expected.update(Sha256::digest(content));
        expected.update(b"\n");
    }
    assert_eq!(
        skill_tree_digest(temp.path()).unwrap(),
        hex(&expected.finalize())
    );
    std::os::unix::fs::symlink(temp.path().join("a.txt"), temp.path().join("alias")).unwrap();
    assert!(skill_tree_digest(temp.path()).is_err());
}

#[test]
fn hashing_rejects_oversized_files_parent_traversal_and_link_components() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let file = root.join("runtime");
    fs::write(&file, b"runtime").unwrap();
    let (digest, size) = regular_file_digest(&file).unwrap();
    assert_eq!(size, 7);
    assert_eq!(hex(&digest), storage::digest(b"runtime"));
    fs::create_dir(root.join("nested")).unwrap();
    assert!(regular_file_digest(&root.join("nested/../runtime")).is_err());
    std::os::unix::fs::symlink(root, root.join("alias")).unwrap();
    assert!(regular_file_digest(&root.join("alias/runtime")).is_err());
    fs::OpenOptions::new()
        .write(true)
        .open(&file)
        .unwrap()
        .set_len(256 * 1024 * 1024 + 1)
        .unwrap();
    assert!(
        regular_file_digest(&file)
            .unwrap_err()
            .to_string()
            .contains("256 MiB")
    );
    let socket = root.join("socket");
    let _socket = std::os::unix::net::UnixListener::bind(&socket).unwrap();
    assert!(regular_file_digest(&socket).is_err());
}

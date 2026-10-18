use super::*;
#[test]
fn observation_must_identify_the_exact_owned_window() {
    let window = json!({"observation":{"windows":[{"id":"0x200003","title":"owned"}]}});
    require_owned_window(&window, "owned", 0x200003).unwrap();
    assert!(require_owned_window(&window, "owned", 0x200004).is_err());
    assert!(require_owned_window(&window, "personal", 0x200003).is_err());
    let duplicate =
        json!({"observation":{"windows":[{"id":"2","title":"owned"},{"id":"3","title":"owned"}]}});
    assert!(require_owned_window(&duplicate, "owned", 2).is_err());
}
#[test]
fn cleanup_cancels_only_its_own_process_group() {
    let directory = tempfile::tempdir().unwrap();
    let mut unrelated = Command::new("/bin/sleep").arg("30").spawn().unwrap();
    let owned = OwnedChild::spawn(
        Path::new("/bin/sleep"),
        &["30".into()],
        directory.path(),
        &[],
        "owned",
    )
    .unwrap();
    let id = owned.child.id();
    drop(owned);
    assert!(!Path::new(&format!("/proc/{id}")).exists());
    assert!(unrelated.try_wait().unwrap().is_none());
    unrelated.kill().unwrap();
    unrelated.wait().unwrap();
}
#[test]
fn rejects_manifest_tampering_and_unsafe_output_layout() {
    let directory = tempfile::tempdir().unwrap();
    let dependencies = directory.path().join("tools/desktop");
    fs::create_dir_all(&dependencies).unwrap();
    assert!(DesktopSession::start(&directory.path().join("outside"), &dependencies).is_err());
    storage::write(
        &dependencies.join("manifest.json"),
        &json!({"source":"authenticated-apt-index","files":{"../outside":"digest"}}),
    )
    .unwrap();
    assert!(verify_dependencies(&dependencies).is_err());
    fs::create_dir_all(dependencies.join("usr/bin")).unwrap();
    fs::create_dir_all(dependencies.join("usr/lib")).unwrap();
    let mut files = serde_json::Map::new();
    for binary in ["Xvfb", "xdotool", "scrot", "openbox"] {
        let path = format!("usr/bin/{binary}");
        fs::write(dependencies.join(&path), b"fixture executable").unwrap();
        files.insert(path, json!(storage::digest(b"fixture executable")));
    }
    let link = dependencies.join("usr/lib/example.so");
    std::os::unix::fs::symlink("../bin/Xvfb", &link).unwrap();
    storage::write(
        &dependencies.join("manifest.json"),
        &json!({"source":"authenticated-apt-index",
            "files":files,"symlinks":{"usr/lib/example.so":"../bin/Xvfb"}}),
    )
    .unwrap();
    verify_dependencies(&dependencies).unwrap();
    fs::remove_file(&link).unwrap();
    std::os::unix::fs::symlink("../bin/xdotool", &link).unwrap();
    assert!(verify_dependencies(&dependencies).is_err());
    fs::remove_file(&link).unwrap();
    std::os::unix::fs::symlink("../bin/Xvfb", &link).unwrap();
    fs::write(dependencies.join("usr/bin/Xvfb"), b"changed bytes").unwrap();
    assert!(verify_dependencies(&dependencies).is_err());
}
#[test]
#[ignore = "set LAB_DESKTOP_DEPENDENCIES and LAB_COMPUTER_USE_BIN for an actual dedicated desktop"]
fn actual_desktop_observation_input_and_cleanup() {
    let dependencies = PathBuf::from(std::env::var("LAB_DESKTOP_DEPENDENCIES").unwrap());
    let cli = PathBuf::from(std::env::var("LAB_COMPUTER_USE_BIN").unwrap());
    let runs = dependencies
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("runs");
    fs::create_dir_all(&runs).unwrap();
    let output = runs.join(format!(
        "desktop-integration-{}-{}",
        std::process::id(),
        workflow::now_ms().unwrap()
    ));
    let mut session = DesktopSession::start(&output, &dependencies).unwrap();
    session.inspect(&cli).unwrap();
    if let Ok(wait_file) = std::env::var("LAB_DESKTOP_INSPECTION_GATE") {
        println!(
            "inspect screenshot: {}",
            output.join("before.png").display()
        );
        storage::write(
            Path::new(&wait_file).with_extension("ready.json").as_path(),
            &json!({"output":output}),
        )
        .unwrap();
        let deadline = Instant::now() + Duration::from_secs(120);
        while !Path::new(&wait_file).exists() {
            assert!(Instant::now() < deadline, "inspection gate timed out");
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    assert_eq!(
        session.verify(&cli, "lab-owned-42").unwrap()["verified"],
        true
    );
    let ids: Vec<_> = session
        .children
        .iter()
        .map(|child| child.child.id())
        .collect();
    session.stop().unwrap();
    assert!(
        ids.iter()
            .all(|id| !Path::new(&format!("/proc/{id}")).exists())
    );
    println!("real desktop evidence: {}", output.display());
}

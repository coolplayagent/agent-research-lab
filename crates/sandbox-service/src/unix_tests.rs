use super::*;
#[test]
fn sandbox_mounts_are_read_only_and_work_is_isolated() {
    let args = arguments(&["/usr/bin/true".into()], &[], false, 4).unwrap();
    assert!(args.contains(&"--unshare-all".into()));
    assert!(!args.contains(&"--share-net".into()));
    assert!(args.windows(2).any(|w| w == ["--tmpfs", "/workspace"]));
    assert!(!args.contains(&"/root".into()));
}

async fn python(state: &Path, script: &str) -> Result<SandboxOutput> {
    run(
        state,
        vec!["/usr/bin/python3".into(), "-c".into(), script.into()],
        vec![],
        vec![],
        false,
        String::new(),
        5,
    )
    .await
}

#[tokio::test]
async fn native_runtime_files_are_bounded_separately_from_protocol_output() {
    let state = tempfile::tempdir().unwrap();
    let result = python(
            state.path(),
            "with open('/workspace/runtime.db','wb') as f: f.write(b'x'*(8*1024*1024))\nprint('runtime file accepted')",
        )
        .await
        .unwrap();
    assert_eq!(result.stdout, "runtime file accepted\n");
    assert!(
        python(
            state.path(),
            "with open('/workspace/runtime.db','wb') as f: f.write(b'x'*(16*1024*1024+1))"
        )
        .await
        .is_err()
    );
    let oversized = python(
        state.path(),
        "import sys; sys.stdout.write('x'*(256*1024+1))",
    )
    .await
    .err()
    .unwrap();
    assert!(
        oversized
            .to_string()
            .contains("protocol output exceeds bound")
    );
    let diagnostics = python(
        state.path(),
        "import sys; sys.stderr.write('x'*(4*1024*1024+1))",
    )
    .await
    .err()
    .unwrap();
    assert!(
        diagnostics
            .to_string()
            .contains("diagnostic output exceeds bound")
    );
    assert!(python(state.path(), "raise SystemExit(7)").await.is_err());
}

#[tokio::test]
async fn private_tmp_does_not_hide_explicit_read_only_runtime_files() {
    let state = tempfile::tempdir().unwrap();
    let runtime = tempfile::Builder::new()
        .prefix("oh-runtime-mount-")
        .tempdir_in("/tmp")
        .unwrap();
    let program = runtime.path().join("agent-runtime");
    fs::copy("/usr/bin/true", &program).unwrap();
    let result = run(
        state.path(),
        vec![program.to_string_lossy().into_owned()],
        vec![],
        vec![program.clone()],
        false,
        String::new(),
        5,
    )
    .await
    .unwrap();
    assert_eq!(result.stdout, "");
    let script = format!(
        "import pathlib; p=pathlib.Path({:?}); assert p.read_bytes();\ntry: p.write_bytes(b'overwrite')\nexcept OSError: print('read-only')\nelse: raise AssertionError('runtime became writable')",
        program.to_string_lossy()
    );
    let result = run(
        state.path(),
        vec!["/usr/bin/python3".into(), "-c".into(), script],
        vec![],
        vec![program],
        false,
        String::new(),
        5,
    )
    .await
    .unwrap();
    assert_eq!(result.stdout.trim(), "read-only");
}

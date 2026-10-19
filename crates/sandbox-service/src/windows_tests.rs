use super::*;
fn cmd(script: &str) -> Vec<String> {
    vec![
        PathBuf::from(std::env::var_os("SystemRoot").unwrap())
            .join("System32/cmd.exe")
            .to_string_lossy()
            .into_owned(),
        "/d".into(),
        "/s".into(),
        "/c".into(),
        script.into(),
    ]
}
#[tokio::test]
async fn appcontainer_can_run_but_cannot_read_private_host_files() {
    let state = tempfile::tempdir().unwrap();
    storage::private_dir(state.path()).unwrap();
    probe(state.path()).await.unwrap();
    let secret = state.path().join("host-private.txt");
    fs::write(&secret, "private fixture").unwrap();
    storage::private_file(&secret).unwrap();
    assert!(
        run(
            state.path(),
            cmd(&format!("type \"{}\"", secret.display())),
            vec![],
            vec![],
            false,
            String::new(),
            5
        )
        .await
        .is_err()
    );
    assert!(
        run(
            state.path(),
            cmd(&format!("echo changed>\"{}\"", secret.display())),
            vec![],
            vec![],
            false,
            String::new(),
            5
        )
        .await
        .is_err()
    );
    assert_eq!(fs::read_to_string(secret).unwrap(), "private fixture");
}
#[tokio::test]
async fn timeout_kills_the_job_and_removes_disposable_files() {
    let state = tempfile::tempdir().unwrap();
    storage::private_dir(state.path()).unwrap();
    let started = Instant::now();
    let result = run(
        state.path(),
        cmd("for /L %i in (0,0,1) do @rem bounded fixture"),
        vec![],
        vec![],
        false,
        String::new(),
        1,
    )
    .await;
    assert!(result.is_err());
    assert!(started.elapsed() < Duration::from_secs(8));
    assert_eq!(fs::read_dir(state.path()).unwrap().count(), 0);
}

use super::*;
#[test]
fn sandbox_health_probe() {
    if std::env::var("OUTCOME_HUB_ISOLATED").as_deref() == Ok("1") {
        probe_child().unwrap();
    }
}
fn child_test(name: &str) -> (Vec<String>, Vec<PathBuf>) {
    let binary = std::env::current_exe().unwrap();
    (
        vec![
            binary.to_string_lossy().into_owned(),
            "--exact".into(),
            format!("windows::tests::{name}"),
            "--nocapture".into(),
        ],
        vec![binary],
    )
}
#[test]
fn sandbox_filesystem_probe() {
    if std::env::var("OUTCOME_HUB_ISOLATED").as_deref() != Ok("1") {
        return;
    }
    windows_runtime::require_app_container().unwrap();
    let secret: PathBuf = serde_json::from_reader(std::io::stdin()).unwrap();
    fs::write("writable.txt", "sandbox fixture").unwrap();
    assert_eq!(
        fs::read_to_string("writable.txt").unwrap(),
        "sandbox fixture"
    );
    assert!(
        fs::read(&secret).is_err(),
        "sandbox read a private host file"
    );
    assert!(
        fs::write(&secret, "changed").is_err(),
        "sandbox wrote a private host file"
    );
    println!("host access denied; workspace read and write succeeded");
}
#[test]
fn sandbox_timeout_probe() {
    if std::env::var("OUTCOME_HUB_ISOLATED").as_deref() == Ok("1") {
        windows_runtime::require_app_container().unwrap();
        use std::io::Write;
        println!("sandbox loop started");
        std::io::stdout().flush().unwrap();
        loop {
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}
#[tokio::test]
async fn cancelling_client_work_terminates_blocking_native_execution() {
    let state = tempfile::tempdir().unwrap();
    storage::private_dir(state.path()).unwrap();
    let root = state.path().to_owned();
    let task = tokio::spawn(async move {
        let (command, paths) = child_test("sandbox_timeout_probe");
        run(&root, command, vec![], paths, false, String::new(), 30).await
    });
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let started = fs::read_dir(state.path())
            .unwrap()
            .filter_map(Result::ok)
            .any(|entry| {
                fs::read_to_string(entry.path().join("output.json"))
                    .is_ok_and(|text| text.contains("sandbox loop started"))
            });
        if started {
            break;
        }
        assert!(Instant::now() < deadline, "sandbox child did not start");
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    task.abort();
    assert!(task.await.err().is_some_and(|error| error.is_cancelled()));
    let deadline = Instant::now() + Duration::from_secs(5);
    while fs::read_dir(state.path()).unwrap().next().is_some() {
        assert!(
            Instant::now() < deadline,
            "cancelled execution retained disposable state"
        );
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}
#[tokio::test]
async fn cancellation_during_runtime_preparation_prevents_process_start() {
    let state = tempfile::tempdir().unwrap();
    storage::private_dir(state.path()).unwrap();
    let runtime = tempfile::tempdir().unwrap();
    for index in 0..2048 {
        fs::write(
            runtime.path().join(format!("dependency-{index}")),
            b"fixture",
        )
        .unwrap();
    }
    let (command, mut paths) = child_test("sandbox_timeout_probe");
    paths.push(runtime.path().to_owned());
    let root = state.path().to_owned();
    let task =
        tokio::spawn(
            async move { run(&root, command, vec![], paths, false, String::new(), 30).await },
        );
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let run = fs::read_dir(state.path())
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .find(|path| path.join("runtime").is_dir());
        if let Some(run) = run {
            assert!(
                !run.join("output.json").exists(),
                "preparation blocked cancellation until after process launch"
            );
            break;
        }
        assert!(
            Instant::now() < deadline,
            "runtime preparation did not start"
        );
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
    task.abort();
    assert!(task.await.err().is_some_and(|error| error.is_cancelled()));
    let deadline = Instant::now() + Duration::from_secs(5);
    while fs::read_dir(state.path()).unwrap().next().is_some() {
        assert!(
            Instant::now() < deadline,
            "cancelled preparation retained disposable state"
        );
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}
#[tokio::test]
async fn appcontainer_can_run_but_cannot_read_private_host_files() {
    let state = tempfile::tempdir().unwrap();
    storage::private_dir(state.path()).unwrap();
    probe(state.path()).await.unwrap();
    let secret = state.path().join("host private.txt");
    fs::write(&secret, "private fixture").unwrap();
    storage::private_file(&secret).unwrap();
    let (command, paths) = child_test("sandbox_filesystem_probe");
    let output = run(
        state.path(),
        command,
        vec![],
        paths,
        false,
        serde_json::to_string(&secret).unwrap(),
        10,
    )
    .await
    .unwrap();
    assert!(
        output
            .stdout
            .contains("host access denied; workspace read and write succeeded")
    );
    assert_eq!(fs::read_to_string(secret).unwrap(), "private fixture");
}
#[tokio::test]
async fn timeout_kills_the_job_and_removes_disposable_files() {
    let state = tempfile::tempdir().unwrap();
    storage::private_dir(state.path()).unwrap();
    let started = Instant::now();
    let (command, paths) = child_test("sandbox_timeout_probe");
    let error = run(
        state.path(),
        command,
        vec![],
        paths,
        false,
        String::new(),
        1,
    )
    .await
    .err()
    .expect("the sandbox must time out");
    assert!(error.to_string().contains("timed out"), "{error:#}");
    assert!(started.elapsed() >= Duration::from_secs(1));
    assert!(started.elapsed() < Duration::from_secs(8));
    assert_eq!(fs::read_dir(state.path()).unwrap().count(), 0);
}

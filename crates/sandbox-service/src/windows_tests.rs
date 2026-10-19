use super::*;
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
        loop {
            std::thread::sleep(Duration::from_millis(50));
        }
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

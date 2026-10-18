use super::*;
use serde_json::json;
use std::sync::atomic::{AtomicUsize, Ordering};

fn lease(ttl: u64) -> Value {
    json!({"run_id":"run-1","owner":"test","acquisition_id":"acquire-1",
            "epoch":1,"issued_at_unix_ms":workflow::now_ms().unwrap(),
            "expires_at_unix_ms":workflow::now_ms().unwrap()+ttl})
}

#[test]
fn renews_while_foreground_is_blocked_and_serializes_current_tokens() {
    let directory = tempfile::tempdir().unwrap();
    let w = Workflow::new("unused", directory.path().join("db"), directory.path());
    let ttl_ms = 3_000;
    let original = lease(ttl_ms);
    let authority = Arc::new(Mutex::new(original.clone()));
    let calls = Arc::new(AtomicUsize::new(0));
    let remote = Arc::clone(&authority);
    let renew_calls = Arc::clone(&calls);
    let (renewed, notifications) = mpsc::channel();
    let mut heartbeat = LeaseHeartbeat::start_with(
        original.clone(),
        journal_path(&w, &original).unwrap(),
        Arc::new(move |token, ttl| {
            let mut current = remote.lock().unwrap();
            ensure!(*token == *current, "old token rejected");
            ensure!(
                expiry(token)? > workflow::now_ms()?,
                "expired token rejected"
            );
            current["expires_at_unix_ms"] = json!(workflow::now_ms()? + ttl);
            renew_calls.fetch_add(1, Ordering::SeqCst);
            renewed.send(()).context("foreground notification closed")?;
            Ok(current.clone())
        }),
        ttl_ms,
        Duration::from_millis(30),
        Duration::from_millis(300),
    )
    .unwrap();
    // Observe renewal notifications instead of assuming a short sleep schedules
    // the background thread. Keep real expiry checks, with room for CI I/O.
    let notification_deadline = Instant::now() + Duration::from_secs(10);
    for _ in 0..3 {
        notifications
            .recv_timeout(notification_deadline.saturating_duration_since(Instant::now()))
            .expect("background renewal must progress without foreground polling");
    }
    // The original authority expires while the foreground makes no lease calls.
    thread::sleep(Duration::from_millis(
        expiry(&original)
            .unwrap()
            .saturating_sub(workflow::now_ms().unwrap())
            + 100,
    ));
    assert!(expiry(&original).unwrap() < workflow::now_ms().unwrap());
    assert!(calls.load(Ordering::SeqCst) >= 3);
    heartbeat
        .with_lease(|token| {
            assert_eq!(*token, *authority.lock().unwrap());
            assert_ne!(*token, original);
            thread::sleep(Duration::from_millis(50));
            assert_eq!(*token, *authority.lock().unwrap());
            Ok(())
        })
        .unwrap();
    let final_token = heartbeat.stop().unwrap();
    let mut recovered = original;
    restore(&w, &mut recovered).unwrap();
    assert_eq!(final_token, recovered);
    assert_eq!(final_token, *authority.lock().unwrap());
    let count = calls.load(Ordering::SeqCst);
    thread::sleep(Duration::from_millis(80));
    assert_eq!(count, calls.load(Ordering::SeqCst));
}

#[test]
fn renewal_failure_kills_worker_without_foreground_polling() {
    let directory = tempfile::tempdir().unwrap();
    let mut worker = process::Process::spawn(
        "sleep",
        &["30".into()],
        directory.path(),
        &directory.path().join("worker"),
        Duration::from_secs(30),
        &[],
        None,
    )
    .unwrap();
    let stat = fs::read_to_string(format!("/proc/{}/stat", worker.pid())).unwrap();
    let start = stat
        .rsplit_once(") ")
        .unwrap()
        .1
        .split_whitespace()
        .nth(19)
        .unwrap()
        .to_owned();
    let heartbeat = LeaseHeartbeat::start_with(
        lease(300),
        directory.path().join("lease.json"),
        Arc::new(|_, _| anyhow::bail!("database unavailable")),
        300,
        Duration::from_millis(30),
        Duration::from_millis(30),
    )
    .unwrap();
    heartbeat
        .watch_worker(worker.pid(), start, Duration::from_secs(30), None)
        .unwrap();
    thread::sleep(Duration::from_millis(200));
    let status = worker
        .poll()
        .unwrap()
        .expect("worker must stop independently");
    assert!(!status.success());
    assert!(
        heartbeat
            .failure()
            .unwrap()
            .contains("database unavailable")
    );
    assert!(heartbeat.with_lease(|_| Ok(())).is_err());
}

#[test]
fn recovery_rejects_other_authority_or_older_tokens() {
    let directory = tempfile::tempdir().unwrap();
    let w = Workflow::new("unused", directory.path().join("db"), directory.path());
    let original = lease(300);
    let mut updated = original.clone();
    updated["epoch"] = json!(2);
    storage::write(&journal_path(&w, &original).unwrap(), &updated).unwrap();
    assert!(restore(&w, &mut original.clone()).is_err());
    updated = original.clone();
    updated["expires_at_unix_ms"] = json!(expiry(&original).unwrap() - 1);
    storage::write(&journal_path(&w, &original).unwrap(), &updated).unwrap();
    assert!(restore(&w, &mut original.clone()).is_err());
}

#[test]
fn watchdog_enforces_timeout_and_pause_while_renewal_blocks() {
    for pause_requested in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let mut worker = process::Process::spawn(
            "sleep",
            &["30".into()],
            directory.path(),
            &directory.path().join("worker"),
            Duration::from_secs(30),
            &[],
            None,
        )
        .unwrap();
        let stat = fs::read_to_string(format!("/proc/{}/stat", worker.pid())).unwrap();
        let start = stat
            .rsplit_once(") ")
            .unwrap()
            .1
            .split_whitespace()
            .nth(19)
            .unwrap()
            .to_owned();
        let heartbeat = LeaseHeartbeat::start_with(
            lease(3_000),
            directory.path().join("lease.json"),
            Arc::new(|old, ttl| {
                // Simulate a slow transport holding the authority mutex.
                thread::sleep(Duration::from_millis(400));
                let mut next = old.clone();
                next["expires_at_unix_ms"] = json!(workflow::now_ms()? + ttl);
                Ok(next)
            }),
            3_000,
            Duration::from_millis(20),
            Duration::from_millis(500),
        )
        .unwrap();
        let pause = directory.path().join("paused");
        heartbeat
            .watch_worker(
                worker.pid(),
                start,
                if pause_requested {
                    Duration::from_secs(30)
                } else {
                    Duration::from_millis(60)
                },
                Some(pause.clone()),
            )
            .unwrap();
        thread::sleep(Duration::from_millis(40));
        if pause_requested {
            fs::write(pause, "pause requested").unwrap();
        }
        thread::sleep(Duration::from_millis(150));
        assert!(
            !worker
                .poll()
                .unwrap()
                .expect("watchdog cancels independently")
                .success()
        );
        let reason = heartbeat.failure().unwrap();
        assert!(reason.contains(if pause_requested { "pause" } else { "timeout" }));
        // Cancellation does not discard valid authority for an unknown-effect observation.
        heartbeat.with_lease(|_| Ok(())).unwrap();
    }
}

#[test]
#[ignore = "set LAB_WORKFLOW_BIN for real workflow renew/finish/release integration"]
fn actual_workflow_survives_blocked_foreground_and_rejects_stale_token() {
    let directory = tempfile::tempdir().unwrap();
    let w = Workflow::new(
        std::env::var("LAB_WORKFLOW_BIN").expect("LAB_WORKFLOW_BIN"),
        directory.path().join("runs.db"),
        directory.path(),
    );
    w.initialize().unwrap();
    w.start_task(
        "long-read",
        false,
        json!({"command":"/bin/sleep 4"}),
        15_000,
    )
    .unwrap();
    let lease = w
        .acquire("long-read", "heartbeat-test", "acquisition-1", 2_000)
        .unwrap();
    let renewer = w.clone();
    let mut heartbeat = LeaseHeartbeat::start_with(
        lease.clone(),
        journal_path(&w, &lease).unwrap(),
        Arc::new(move |lease, ttl| renewer.renew(lease, ttl)),
        2_000,
        Duration::from_millis(200),
        Duration::from_millis(500),
    )
    .unwrap();
    let claim = heartbeat.with_lease(|lease| w.claim(lease)).unwrap();
    let output = process::capture(
        "/bin/sleep",
        &["4.5".into()],
        directory.path(),
        Duration::from_secs(6),
    )
    .unwrap();
    assert!(output.status.success());
    assert!(
        w.release(&lease).is_err(),
        "real workflow must reject original stale token"
    );
    let receipt = heartbeat
        .with_lease(|lease| {
            w.finish(lease, &claim["attempt"],
            json!({"result":"actual sleep exited successfully across multiple lease lifetimes"}))
        })
        .unwrap();
    assert_eq!(receipt["snapshot"]["status"], "succeeded");
    let current = heartbeat.stop().unwrap();
    w.release(&current).unwrap();
    w.verify("long-read").unwrap();
}

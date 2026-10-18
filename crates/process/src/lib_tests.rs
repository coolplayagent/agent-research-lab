use super::*;
#[test]
fn literal_args_and_exit_status() {
    let t = tempfile::tempdir().unwrap();
    let o = capture(
        "printf",
        &["%s".into(), "$(touch forbidden)".into()],
        t.path(),
        Duration::from_secs(2),
    )
    .unwrap();
    assert_eq!(o.stdout, b"$(touch forbidden)");
    assert!(!t.path().join("forbidden").exists());
    assert!(
        !capture("false", &[], t.path(), Duration::from_secs(2))
            .unwrap()
            .status
            .success()
    );
}
#[test]
fn fast_successful_exit_does_not_bypass_output_limit() {
    let temp = tempfile::tempdir().unwrap();
    let mut process = Process::spawn(
        "/usr/bin/head",
        &[
            "-c".into(),
            (MAX_OUTPUT + 1).to_string(),
            "/dev/zero".into(),
        ],
        temp.path(),
        &temp.path().join("logs"),
        Duration::from_secs(5),
        &[],
        None,
    )
    .unwrap();
    let started = Instant::now();
    while process.child.try_wait().unwrap().is_none() {
        assert!(started.elapsed() < Duration::from_secs(5));
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        process
            .poll()
            .unwrap_err()
            .to_string()
            .contains("output limit")
    );
}
#[test]
fn deadline_terminates_child() {
    let t = tempfile::tempdir().unwrap();
    assert!(capture("sleep", &["5".into()], t.path(), Duration::from_millis(20)).is_err());
}

#[path = "observer_tests.rs"]
pub(super) mod observer_tests;

mod parallel_tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn independent_checks_are_bounded_and_return_in_input_order() {
        let active = AtomicUsize::new(0);
        let peak = AtomicUsize::new(0);
        let items: Vec<_> = (0..11).collect();
        let result = parallel_map(&items, |item| {
            let current = active.fetch_add(1, Ordering::SeqCst) + 1;
            peak.fetch_max(current, Ordering::SeqCst);
            std::thread::sleep(Duration::from_millis(20 + (3 - item % 4) * 5));
            active.fetch_sub(1, Ordering::SeqCst);
            Ok(item * 2)
        })
        .unwrap();
        assert_eq!(
            result,
            items.iter().map(|item| item * 2).collect::<Vec<_>>()
        );
        assert!((2..=4).contains(&peak.load(Ordering::SeqCst)));
        assert_eq!(active.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn failures_and_panics_join_started_siblings_without_starting_later_batches() {
        for panic in [false, true] {
            let completed = AtomicUsize::new(0);
            let started = AtomicUsize::new(0);
            let result = parallel_map(&[0, 1, 2, 3, 4], |item| {
                started.fetch_add(1, Ordering::SeqCst);
                if *item == 0 {
                    assert!(!panic, "fixture worker panic");
                    bail!("fixture unavailable source");
                }
                std::thread::sleep(Duration::from_millis(30));
                completed.fetch_add(1, Ordering::SeqCst);
                Ok(())
            });
            assert!(result.is_err());
            assert_eq!(started.load(Ordering::SeqCst), 4);
            assert_eq!(completed.load(Ordering::SeqCst), 3);
        }
    }

    #[test]
    fn later_batches_share_the_original_host_deadline() {
        let temp = tempfile::tempdir().unwrap();
        let started = Instant::now();
        let _deadline = deadline_scope(Duration::from_millis(250));
        let calls = AtomicUsize::new(0);
        let result = parallel_map(&[0, 1, 2, 3, 4, 5, 6, 7, 8], |item| {
            calls.fetch_add(1, Ordering::SeqCst);
            capture(
                "/bin/sleep",
                &[if *item < 4 { "0.10" } else { "5" }.into()],
                temp.path(),
                Duration::from_secs(10),
            )
        });
        assert!(result.is_err());
        assert!(deadline_exhausted());
        assert!(started.elapsed() < Duration::from_secs(2));
        assert!(calls.load(Ordering::SeqCst) <= 8);
        assert!(parallel_map(&[1], |_| -> Result<()> { panic!("expired work started") }).is_err());
    }
}

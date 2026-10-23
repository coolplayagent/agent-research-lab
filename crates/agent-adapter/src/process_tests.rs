#[cfg(unix)]
#[test]
fn abnormal_exit_retains_the_signal_or_exit_status() {
    use super::*;
    let root = tempfile::tempdir().unwrap();
    for (script, reason) in [("exit 7", "7"), ("kill -XFSZ $$", "SIGXFSZ")] {
        let mut session = Session::spawn(
            Path::new("/bin/sh"),
            &["-c".into(), script.into()],
            root.path(),
            &[],
            3,
            false,
        )
        .unwrap();
        session.close_input();
        while session.next().unwrap().is_some() {}
        let error = session.wait().unwrap_err().to_string();
        assert!(error.contains(reason), "{error}");
        assert!(!error.contains("exit code None"));
    }
}

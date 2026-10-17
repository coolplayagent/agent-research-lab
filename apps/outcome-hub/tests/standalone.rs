//! Exercise the shipped application without a research config, model CLI or knowledge service.
use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

struct Process(Child);
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn get(addr: SocketAddr, path: &str) -> std::io::Result<String> {
    let mut stream = TcpStream::connect_timeout(&addr, Duration::from_millis(200))?;
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    write!(
        stream,
        "GET {path} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n"
    )?;
    let mut reply = String::new();
    stream.read_to_string(&mut reply)?;
    Ok(reply)
}
#[test]
fn core_binary_starts_and_restarts_without_optional_services() {
    let root = tempfile::tempdir().unwrap();
    for _ in 0..2 {
        let reservation = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = reservation.local_addr().unwrap();
        drop(reservation);
        let child = Command::new(env!("CARGO_BIN_EXE_ai-im"))
            .args(["serve", "--state-dir"])
            .arg(root.path())
            .args([
                "--listen",
                &addr.to_string(),
                "--max-seconds",
                "3",
                "--local-operator",
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let mut child = Process(child);
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if let Ok(page) = get(addr, "/") {
                assert!(page.starts_with("HTTP/1.1 200"));
                break;
            }
            assert!(
                child.0.try_wait().unwrap().is_none(),
                "core exited before readiness"
            );
            assert!(Instant::now() < deadline, "core did not become ready");
            std::thread::sleep(Duration::from_millis(20));
        }
        let response = get(addr, "/api/im/scenarios").unwrap();
        assert!(response.starts_with("HTTP/1.1 200"));
        let body: serde_json::Value =
            serde_json::from_str(response.split_once("\r\n\r\n").unwrap().1).unwrap();
        let scenarios = body["scenarios"].as_array().unwrap();
        assert!(scenarios.iter().any(|s| s["id"] == "collaboration"));
        assert!(scenarios.iter().all(|s| s["id"] != "cli-research"));
        let topics = get(addr, "/api/im/topics").unwrap();
        assert!(topics.starts_with("HTTP/1.1 200"));
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(status) = child.0.try_wait().unwrap() {
                assert!(status.success());
                break;
            }
            assert!(
                Instant::now() < deadline,
                "core exceeded its bounded lifetime"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }
    assert!(root.path().join("crystal").is_dir());
    assert!(!root.path().join("jobs").exists());
}

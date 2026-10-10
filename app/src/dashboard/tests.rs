use super::*;

fn job() -> Job {
    serde_json::from_value(json!({
        "task":{"id":"research-one","role":"research","repository":"superpod","prompt":"PRIVATE_PROMPT",
            "dependencies":["predecessor"]},
        "model":"test-model","source_commit":"a".repeat(40),"superpod_commit":"b".repeat(40),
        "prompt_digest":"c".repeat(64),"config_digest":"d".repeat(64),
        "worktree":"/PRIVATE_WORKTREE","run_id":"research-one-attempt-1","attempt":1,"last_error":null,
        "launch":{"pid":123,"process_start":"1","lease":{"token":"PRIVATE_LEASE"},
            "attempt":{},"log_dir":"/PRIVATE_LOG","tools":{"credential":"PRIVATE_TOOL"}}
    })).unwrap()
}

#[test]
fn projection_uses_committed_context_without_exposing_raw_worker_data() {
    let receipt = json!({"communication":{"context":{"message_ids":["message-one"],"digest":"context-digest","as_of":42,"text":"PRIVATE_CONTEXT"}},
        "agent_report":{"summary":"PUBLIC_REPORT"},"candidate":{"candidate_commit":"e".repeat(40)}});
    let row = json!({"state":"succeeded","error":null,"status":{"frames":{"1":{"nodes":{"task":{"outputs":{"result":receipt.to_string()}}}}}}});
    let view = project(&job(), &row);
    assert_eq!(view["state"], "succeeded");
    assert_eq!(view["report"]["summary"], "PUBLIC_REPORT");
    assert_eq!(view["context"]["message_ids"], json!(["message-one"]));
    assert_eq!(view["dependencies"], json!(["predecessor"]));
    assert_eq!(view["candidate_commit"], "e".repeat(40));
    assert!(!view.to_string().contains("PRIVATE_"));
}

#[test]
fn http_rejects_dns_rebinding_foreign_origins_mutation_and_duplicate_hosts() {
    let addr = "127.0.0.1:8090".parse().unwrap();
    assert_eq!(allowed_request("GET /api/snapshot HTTP/1.1\r\nHost: localhost:8090\r\nSec-Fetch-Site: same-origin\r\n\r\n",addr).unwrap(), "/api/snapshot");
    for request in [
        "GET /api/snapshot HTTP/1.1\r\nHost: attacker.example:8090\r\n\r\n",
        "GET /api/snapshot HTTP/1.1\r\nHost: 127.0.0.1:8090\r\nOrigin: https://attacker.example\r\n\r\n",
        "GET /api/snapshot HTTP/1.1\r\nHost: 127.0.0.1:8090\r\nSec-Fetch-Site: cross-site\r\n\r\n",
        "GET / HTTP/1.1\r\nHost: 127.0.0.1:8090\r\nHost: localhost:8090\r\n\r\n",
        "POST /api/snapshot HTTP/1.1\r\nHost: 127.0.0.1:8090\r\n\r\n",
    ] {
        assert!(allowed_request(request, addr).is_err());
    }
}

#[test]
fn actual_http_serves_embedded_assets_and_never_arbitrary_paths() {
    for (path, status, content) in [
        ("/", "200 OK", "水晶球"),
        ("/api/snapshot", "200 OK", "sample-marker"),
        ("/../local.toml", "404 Not Found", "Not found"),
        (
            "/api/session/unknown-attempt-1",
            "404 Not Found",
            "Unknown session",
        ),
        (
            "/api/session/../../local.toml",
            "404 Not Found",
            "Unknown session",
        ),
        (
            "/api/profile/../../local.toml",
            "404 Not Found",
            "Unknown profile",
        ),
        ("/api/profile/unknown", "404 Not Found", "Unknown profile"),
    ] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let shared = Arc::new(RwLock::new(json!({"sample":"sample-marker"})));
        thread::scope(|scope| {
            scope.spawn(|| {
                let (stream, _) = listener.accept().unwrap();
                handle(
                    stream,
                    addr,
                    &shared,
                    Path::new("/unused"),
                    Path::new("/unused"),
                    &AtomicBool::new(false),
                )
                .unwrap();
            });
            let mut client = TcpStream::connect(addr).unwrap();
            client
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            write!(client, "GET {path} HTTP/1.1\r\nHost: {addr}\r\n\r\n").unwrap();
            let mut result = String::new();
            client.read_to_string(&mut result).unwrap();
            assert!(result.starts_with(&format!("HTTP/1.1 {status}")));
            assert!(result.contains(content));
            assert!(result.contains("frame-ancestors 'none'"));
        });
    }
}

#[test]
fn live_events_deliver_revisions_while_snapshot_requests_remain_available() {
    use std::io::{BufRead, BufReader};
    fn next_snapshot(client: &mut BufReader<TcpStream>) -> String {
        loop {
            let mut line = String::new();
            assert!(client.read_line(&mut line).unwrap() > 0);
            if line.starts_with("data: ") {
                return line;
            }
        }
    }
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let shared = Arc::new(RwLock::new(json!({"revision":1,"sample":"first"})));
    let stop = AtomicBool::new(false);
    thread::scope(|scope| {
        scope.spawn(|| {
            for _ in 0..2 {
                let (stream, _) = listener.accept().unwrap();
                let shared = &shared;
                let stop = &stop;
                scope.spawn(move || {
                    handle(
                        stream,
                        addr,
                        shared,
                        Path::new("/unused"),
                        Path::new("/unused"),
                        stop,
                    )
                    .unwrap()
                });
            }
        });
        let mut stream = TcpStream::connect(addr).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        write!(stream, "GET /api/events HTTP/1.1\r\nHost: {addr}\r\n\r\n").unwrap();
        let mut events = BufReader::new(stream);
        assert!(next_snapshot(&mut events).contains("first"));
        let mut client = TcpStream::connect(addr).unwrap();
        client
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        write!(client, "GET /api/snapshot HTTP/1.1\r\nHost: {addr}\r\n\r\n").unwrap();
        let mut result = String::new();
        client.read_to_string(&mut result).unwrap();
        assert!(result.starts_with("HTTP/1.1 200 OK"));
        assert!(result.contains("first"));
        *shared.write().unwrap() = json!({"revision":2,"sample":"second"});
        assert!(next_snapshot(&mut events).contains("second"));
        stop.store(true, Ordering::Relaxed);
    });
}

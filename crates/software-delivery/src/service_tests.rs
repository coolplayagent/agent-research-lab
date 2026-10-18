use super::*;

#[test]
fn platform_binary_replacement_preserves_endpoints_and_environment() {
    let previous = "[Service]\nEnvironmentFile=/private/service.env\nExecStart=/old/ai-im-storage --state-dir /durable/store --socket /private/storage.sock --max-seconds 43200\nRestart=always\n";
    let changed = replace_executable(previous, Path::new("/new/ai-im-storage")).unwrap();
    assert!(changed.contains("EnvironmentFile=/private/service.env"));
    assert!(changed.contains("ExecStart=/new/ai-im-storage --state-dir /durable/store --socket /private/storage.sock --max-seconds 43200"));
    assert!(replace_executable("ExecStart=/one\nExecStart=/two", Path::new("/new")).is_err());
}

#[test]
fn health_checks_use_actual_http_status_and_exact_host() {
    for (status, succeeds) in [
        ("200 OK", true),
        ("401 Unauthorized", false),
        ("503 Unavailable", false),
    ] {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut request = Vec::new();
            while !request.windows(4).any(|bytes| bytes == b"\r\n\r\n") {
                let mut buffer = [0u8; 256];
                let count = stream.read(&mut buffer).unwrap();
                assert!(count > 0 && request.len() + count <= 1024);
                request.extend_from_slice(&buffer[..count]);
            }
            assert!(String::from_utf8_lossy(&request).contains(&format!("Host: {address}\r\n")));
            write!(
                stream,
                "HTTP/1.1 {status}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            )
            .unwrap();
        });
        assert_eq!(http_health(&format!("http://{address}/")).is_ok(), succeeds);
        server.join().unwrap();
    }
}

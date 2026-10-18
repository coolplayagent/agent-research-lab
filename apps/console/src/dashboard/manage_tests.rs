use super::*;
#[test]
fn mutations_require_exact_origin_intent_json_and_bounded_length() {
    let addr = "127.0.0.1:8090".parse().unwrap();
    let valid = "POST /api/people HTTP/1.1\r\nHost: 127.0.0.1:8090\r\nOrigin: http://127.0.0.1:8090\r\nContent-Type: application/json\r\nX-Crystal-Intent: manage-people\r\nContent-Length: 24\r\n\r\n";
    assert_eq!(content_length(valid, addr).unwrap(), 24);
    for request in [
        valid.replace("Origin: http://127.0.0.1:8090\r\n", ""),
        valid.replace("application/json", "text/plain"),
        valid.replace("manage-people", "run"),
        valid.replace("Length: 24", "Length: 20000"),
        valid.replace(
            "Origin: http://127.0.0.1:8090",
            "Origin: https://foreign.example",
        ),
        valid.replace(
            "Content-Length: 24",
            "Transfer-Encoding: chunked\r\nContent-Length: 24",
        ),
        valid.replace(
            "Content-Length: 24",
            "Content-Length: 24\r\nContent-Length: 24",
        ),
        valid.replace("/api/people", "/api/enqueue"),
    ] {
        assert!(content_length(&request, addr).is_err());
    }
}

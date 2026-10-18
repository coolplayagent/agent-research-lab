use super::*;

#[test]
fn legacy_operation_bytes_and_queue_states_remain_compatible() {
    let wire = r#"{"kind":"knowledge_refresh","request":{"relay_knowledge_binary":"/tool/relay-knowledge","superpod_root":"/knowledge","repository_alias":"superpod","expected_commit":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}}"#;
    let operation: Operation = serde_json::from_str(wire).unwrap();
    assert_eq!(serde_json::to_string(&operation).unwrap(), wire);
    let aliased: Operation =
        serde_json::from_str(&wire.replace("superpod_root", "knowledge_root")).unwrap();
    assert_eq!(serde_json::to_string(&aliased).unwrap(), wire);
    for state in QueueState::ALL {
        let wire = format!(
            r#"{{"id":"{}","state":"{state}","attempts":2,"request_path":"/outbox/requests/request.json","result":{{"retained":true}},"error":null,"updated_at":"2026-10-18T00:00:00Z","retry_after_unix":30}}"#,
            "a".repeat(64)
        );
        let entry: QueueEntry = serde_json::from_str(&wire).unwrap();
        assert_eq!(serde_json::to_string(&entry).unwrap(), wire);
    }
}

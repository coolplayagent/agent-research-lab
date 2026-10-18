use super::*;
use crate::authority::message_id;

#[test]
fn legacy_public_proposal_bytes_preserve_frozen_proposal_and_message_identity() {
    // This v1 wire fixture predates recipient routing; omission must stay canonical.
    let legacy = r#"{"schema_version":1,"id":"p1","kind":"finding","text":"Frozen observation","topics":["recovery"],"references":[],"reply_to":null}"#;
    let proposal: Proposal = serde_json::from_str(legacy).unwrap();
    proposal.validate().unwrap();
    assert!(proposal.recipients.is_empty());
    assert_eq!(serde_json::to_string(&proposal).unwrap(), legacy);
    let digest = storage::digest(legacy.as_bytes());
    assert_eq!(
        digest,
        "352cad4050656c3c7f516807966c3cb05148b396a2f719e6e24197c4127f6ca8"
    );
    let member = HostMember {
        cohort_id: format!("{:064x}", 1),
        task_id: "task".into(),
        run_id: "run".into(),
        authority_sha256: "a".repeat(64),
    };
    assert_eq!(
        message_id(&member.cohort_id, &member, 0, &digest).unwrap(),
        "e6b5de8b605548eb50ccae76264a2974594bb56f630cf49493347de88df8d797"
    );
    let mut explicit_empty: serde_json::Value = serde_json::from_str(legacy).unwrap();
    explicit_empty["recipients"] = serde_json::json!([]);
    let decoded: Proposal = serde_json::from_value(explicit_empty).unwrap();
    assert_eq!(serde_json::to_string(&decoded).unwrap(), legacy);
}

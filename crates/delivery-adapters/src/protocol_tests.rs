use super::*;

#[test]
fn legacy_install_receipt_fields_and_phase_strings_keep_exact_json_bytes() {
    // Shape-only historical data: decoding a journal is not evidence validation.
    let legacy = r#"{"operation_id":"op","target":"/skills/tool","backup":null,"previous_sha256":null,"package_sha256":"package","binary_sha256":"binary","binary_relative_path":"cli","version":"v1","evidence":{"source_commit":"source","upstream":null,"skills_manifest":null,"prompt_sha256":"prompt","policy_sha256":"policy","superpod_commit":"knowledge","implementer_id":"implementer","evaluator_id":"reviewer","evaluation_passed":true,"qualitygate_passed":true,"qualitygate_report":{"path":"/evidence/gate.json","sha256":"gate"},"snapshot_receipt":{"path":"/evidence/snapshot.json","sha256":"snapshot"},"evaluation_receipt":{"path":"/evidence/review.json","sha256":"review"}},"phase":"staged","receipt_path":"/backups/op/receipt.json"}"#;
    for phase in [
        "staged",
        "previous_backed_up",
        "activated",
        "verified",
        "rolled_back",
    ] {
        let wire = legacy.replace("\"phase\":\"staged\"", &format!("\"phase\":\"{phase}\""));
        let receipt: InstallReceipt = serde_json::from_str(&wire).unwrap();
        assert_eq!(receipt.phase.as_str(), phase);
        assert_eq!(serde_json::to_string(&receipt).unwrap(), wire);
        assert!(receipt.evidence.validate().is_err());
    }
}

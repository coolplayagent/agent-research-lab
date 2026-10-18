use super::*;

#[test]
fn registry_integrity_decoding_rejects_invalid_padding() {
    assert_eq!(
        decode_sha256("47DEQpj8HBSa+/TImW+5JCeuQeRkm5NMpJWZG3hSuFU=").unwrap(),
        storage::digest(b"")
    );
    assert!(decode_sha256("47DEQpj8HBSa+/TImW+5JCeuQeRkm5NMpJWZG3hSuFV=").is_err());
}

#[test]
fn altered_metadata_archives_fail_before_their_members_can_be_read() {
    let temp = tempfile::tempdir().unwrap();
    let (cas, metadata) =
        crate::test_support::verified_tar(temp.path(), &[("metadata", "public metadata")]);
    let archive = verified_archive(&cas, &metadata).unwrap().unwrap();
    native_effects::fs::write(&archive, b"unreviewed replacement").unwrap();
    assert!(
        verified_archive(&cas, &metadata)
            .unwrap_err()
            .to_string()
            .contains("checksum mismatch")
    );
    assert!(
        verified_archive(&cas, &serde_json::json!({"integrity":"unknown-algorithm"}))
            .unwrap()
            .is_none()
    );
}

use super::{RemoteSnapshot, SkillsManifest};
use serde_json::Value;

#[test]
fn retained_protocol_preserves_unicode_optional_proofs_and_unknown_field_rejection() {
    let fixture: Value =
        serde_json::from_str(include_str!("../tests/fixtures/protocol.json")).unwrap();
    let remote: RemoteSnapshot = serde_json::from_value(fixture["remote"].clone()).unwrap();
    remote.validate().unwrap();
    assert_eq!(serde_json::to_value(&remote).unwrap(), fixture["remote"]);
    assert_eq!(
        crate::identity::url_component(&remote.default_branch),
        fixture["branch_url"].as_str().unwrap()
    );
    let skills: SkillsManifest = serde_json::from_value(fixture["skills"].clone()).unwrap();
    assert!(skills.skills[0].source_qualitygate.is_none());
    assert_eq!(serde_json::to_value(&skills).unwrap(), fixture["skills"]);
    let mut changed = fixture["skills"].clone();
    changed["skills"][0]["approved"] = true.into();
    assert!(serde_json::from_value::<SkillsManifest>(changed).is_err());
    let mut changed = fixture["remote"].clone();
    changed["verified"] = true.into();
    assert!(serde_json::from_value::<RemoteSnapshot>(changed).is_err());
}

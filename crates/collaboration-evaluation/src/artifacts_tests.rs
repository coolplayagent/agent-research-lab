use super::*;
use crate::tests::fixture;
use serde_json::json;

#[test]
fn private_artifacts_are_immutable_and_symlink_escape_is_rejected() {
    let (temp, c, plan, _) = fixture();
    let root = c.state_dir.join("private/pilot");
    materialize(&c, &root, &plan).unwrap();
    materialize(&c, &root, &plan).unwrap();
    assert!(immutable_write(&c, &root.join("manifest.json"), &json!({"changed":true})).is_err());
    assert!(immutable_write(&c, &temp.path().join("public.json"), &plan).is_err());
    std::os::unix::fs::symlink(temp.path(), c.state_dir.join("private/escape")).unwrap();
    assert!(immutable_write(&c, &c.state_dir.join("private/escape/leak.json"), &plan).is_err());
}

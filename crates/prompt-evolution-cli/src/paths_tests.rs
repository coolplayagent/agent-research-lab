use super::{ensure_host_path, resolve_destination, resolve_isolation};
use crate::{
    PromotionRequest,
    files::read_strict,
    test_support::{prepare_gate, setup},
};
use std::os::unix::fs::symlink;

#[test]
fn path_aliases_cannot_move_host_evidence_into_exposed_stores() {
    let temp = setup();
    let input = prepare_gate(temp.path(), "evaluate");
    let mut request: PromotionRequest = read_strict(&input).unwrap();
    let host = temp.path().join("host");
    resolve_isolation(&host, &mut request.holdout_isolation);
    request.holdout_isolation.validate().unwrap();
    for name in ["optimizer", "superpod", "memory"] {
        let alias = host.join(name);
        symlink(temp.path().join(name), &alias).unwrap();
        assert!(
            ensure_host_path(&alias.join("new-receipt.json"), &request.holdout_isolation).is_err()
        );
    }
    let target = host.join("context.json");
    let alias = host.join("context-alias.json");
    symlink(&target, &alias).unwrap();
    assert!(resolve_destination(&alias).is_err());
    assert_eq!(
        resolve_destination(&host.join("new-receipt.json")).unwrap(),
        host.join("new-receipt.json")
    );
    assert!(resolve_destination(&host.join("missing/receipt.json")).is_err());
}

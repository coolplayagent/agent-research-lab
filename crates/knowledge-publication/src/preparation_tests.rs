use super::*;
use crate::test_support::preparation_fixture;

#[cfg(unix)]
#[test]
fn preparation_isolated_from_main_checkout_and_retry_safe() {
    let (_temp, request) = preparation_fixture();
    let root = &request.knowledge_root;
    let prepared = prepare_report(&request).unwrap();
    assert!(!root.join(&prepared.report_path).exists());
    assert!(git(root, &["status", "--porcelain"]).unwrap().is_empty());
    assert_eq!(
        prepare_report(&request).unwrap().report_sha256,
        prepared.report_sha256
    );
}

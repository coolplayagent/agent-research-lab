use super::*;
use crate::test_support::publication_fixture;

#[cfg(unix)]
#[test]
fn publication_rejects_forged_main_and_same_base_before_effects() {
    let (_temp, mut request) = publication_fixture();
    request.prepared.branch = "main".into();
    assert!(
        publish_report(&request)
            .unwrap_err()
            .to_string()
            .contains("dedicated outcome branch")
    );
    request.prepared.branch = format!("research/{}", request.prepared.experiment_id);
    request.base_branch = request.prepared.branch.clone();
    assert!(
        publish_report(&request)
            .unwrap_err()
            .to_string()
            .contains("dedicated outcome branch")
    );
    assert_eq!(
        git(&request.prepared.worktree, &["rev-parse", "HEAD"]).unwrap(),
        request.prepared.baseline
    );
}

use super::*;
use crate::RemoteSnapshot;
use chrono::Utc;

#[test]
fn rejects_untrusted_origins_and_compares_identity_without_check_time() {
    assert!(github_origin("https://github.com/other/repo.git").is_err());
    assert!(github_origin("https://token@github.com/coolplayagent/repo.git").is_err());
    assert_eq!(
        github_origin("git@github.com:stevetdp/superpod.git").unwrap(),
        "stevetdp/superpod"
    );
    let first = RemoteSnapshot {
        repository: "coolplayagent/repo".into(),
        default_branch: "main".into(),
        commit: "a".repeat(40),
        checked_at: Utc::now().to_rfc3339(),
    };
    let mut later = first.clone();
    later.checked_at = "2026-10-09T00:00:00Z".into();
    assert!(first.same_binding(&later));
    assert_ne!(first, later);
}

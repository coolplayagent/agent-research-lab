use super::*;

#[test]
fn full_access_is_explicit_and_nested_agents_cannot_escape_the_global_limit() {
    let args = arguments();
    assert!(args.windows(2).any(|p| p == ["-s", "danger-full-access"]));
    assert!(
        args.windows(2)
            .any(|p| p == ["-c", "approval_policy=\"never\""])
    );
    assert!(
        args.windows(2)
            .any(|p| p == ["-c", "features.multi_agent=false"])
    );
}

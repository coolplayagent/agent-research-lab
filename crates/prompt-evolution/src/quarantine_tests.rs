use super::*;

#[test]
fn rule_changes_remain_quarantined_and_content_bound() {
    let proposal = QuarantinedRuleProposal::new(
        "current".into(),
        "threshold=0".into(),
        "isolated experiment".into(),
    )
    .unwrap();
    proposal.validate().unwrap();
    assert!(!proposal.can_adopt_automatically());
    let mut tampered = proposal;
    tampered.proposed_rule = "threshold=-1".into();
    assert!(tampered.validate().is_err());
}

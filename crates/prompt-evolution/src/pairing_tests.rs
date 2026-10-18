use super::*;
use crate::test_support::{context, trials};

#[test]
fn each_evidence_binding_is_required_and_paired_knowledge_cannot_change() {
    let original = trials(3, 1, 2);
    for field in [
        "source_commit",
        "prompt_version",
        "policy_version",
        "superpod_commit",
        "configuration_digest",
    ] {
        let mut observations = original.clone();
        let mut wire = serde_json::to_value(&observations[1]).unwrap();
        wire["snapshot"][field] = serde_json::json!("changed-binding");
        observations[1] = serde_json::from_value(wire).unwrap();
        assert!(paired_trials(&context(), &observations).is_err(), "{field}");
    }
    let mut changed = context();
    changed.candidate.superpod_commit = "different-knowledge".into();
    assert!(paired_trials(&changed, &original).is_err());
}

#[test]
fn non_holdout_partitions_and_cross_family_trial_ids_cannot_be_paired() {
    for partition in [DatasetPartition::Development, DatasetPartition::Validation] {
        let mut observations = trials(3, 1, 2);
        observations[0].partition = partition;
        assert!(paired_trials(&context(), &observations).is_err());
    }
    let mut observations = trials(3, 1, 2);
    observations[6].trial_id = observations[0].trial_id.clone();
    let result = paired_trials(&context(), &observations);
    assert!(result.is_err_and(|error| error.to_string().contains("reused across task families")));
}

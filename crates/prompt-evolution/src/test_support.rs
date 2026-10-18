//! Shared evidence fixtures for domain gate tests.
use crate::{
    DatasetPartition, EvaluationContext, PromptDefinition, PromptVersion, SnapshotBinding,
    TrialObservation, Variant,
};

pub(super) fn prompt(content: &str, parents: Vec<String>) -> PromptVersion {
    PromptVersion::new(PromptDefinition {
        parent_versions: parents,
        role: "researcher".into(),
        task_kind: "long_horizon".into(),
        content: content.into(),
        change_reason: "test a new hypothesis".into(),
        failure_conditions: vec!["critical regression".into()],
    })
    .unwrap()
}

pub(super) fn snapshot(prompt: &str) -> SnapshotBinding {
    SnapshotBinding {
        source_commit: "source-v1".into(),
        prompt_version: prompt.into(),
        policy_version: "policy-v1".into(),
        superpod_commit: "knowledge-v1".into(),
        configuration_digest: "configuration-v1".into(),
    }
}

pub(super) fn context() -> EvaluationContext {
    EvaluationContext {
        baseline: snapshot("base"),
        candidate: snapshot("candidate"),
    }
}

pub(super) fn trials(
    families: usize,
    baseline_successes: usize,
    candidate_successes: usize,
) -> Vec<TrialObservation> {
    let context = context();
    let mut result = Vec::new();
    for family in 0..families {
        for trial in 0..3 {
            for variant in [Variant::Baseline, Variant::Candidate] {
                result.push(TrialObservation {
                    trial_id: format!("{family}-{trial}"),
                    task_family: format!("family-{family}"),
                    partition: DatasetPartition::Holdout,
                    variant,
                    snapshot: if variant == Variant::Baseline {
                        context.baseline.clone()
                    } else {
                        context.candidate.clone()
                    },
                    success: trial
                        < if variant == Variant::Baseline {
                            baseline_successes
                        } else {
                            candidate_successes
                        },
                    duration_ms: 100,
                    tokens: Some(100),
                    critical_regression: false,
                });
            }
        }
    }
    result
}

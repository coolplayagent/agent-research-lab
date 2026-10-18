//! Pure pooled summaries and the unchanged ten-percent efficiency predicate.
use crate::{pairing::Families, protocol::EvaluationMetrics};

fn median(values: impl Iterator<Item = u64>) -> Option<f64> {
    let mut values: Vec<u64> = values.collect();
    if values.is_empty() {
        return None;
    }
    values.sort_unstable();
    let middle = values.len() / 2;
    if values.len().is_multiple_of(2) {
        Some(values[middle - 1] as f64 / 2.0 + values[middle] as f64 / 2.0)
    } else {
        Some(values[middle] as f64)
    }
}

pub(super) fn summarize(families: &Families<'_>) -> EvaluationMetrics {
    let pairs: Vec<_> = families.values().flatten().collect();
    let baseline_successes = pairs.iter().filter(|p| p.baseline.success).count();
    let candidate_successes = pairs.iter().filter(|p| p.candidate.success).count();
    let complete_tokens = !pairs.is_empty()
        && pairs
            .iter()
            .all(|p| p.baseline.tokens.is_some() && p.candidate.tokens.is_some());
    EvaluationMetrics {
        families: families.len(),
        pairs: pairs.len(),
        baseline_successes,
        candidate_successes,
        baseline_success_rate: if pairs.is_empty() {
            0.0
        } else {
            baseline_successes as f64 / pairs.len() as f64
        },
        candidate_success_rate: if pairs.is_empty() {
            0.0
        } else {
            candidate_successes as f64 / pairs.len() as f64
        },
        baseline_median_duration_ms: median(pairs.iter().map(|p| p.baseline.duration_ms))
            .unwrap_or(0.0),
        candidate_median_duration_ms: median(pairs.iter().map(|p| p.candidate.duration_ms))
            .unwrap_or(0.0),
        baseline_median_tokens: complete_tokens
            .then(|| median(pairs.iter().filter_map(|p| p.baseline.tokens)))
            .flatten(),
        candidate_median_tokens: complete_tokens
            .then(|| median(pairs.iter().filter_map(|p| p.candidate.tokens)))
            .flatten(),
    }
}

pub(super) fn improved_efficiency(metrics: &EvaluationMetrics) -> bool {
    let time = metrics.baseline_median_duration_ms > 0.0
        && metrics.candidate_median_duration_ms <= metrics.baseline_median_duration_ms * 0.9;
    let tokens = matches!(
        (metrics.baseline_median_tokens, metrics.candidate_median_tokens),
        (Some(baseline), Some(candidate)) if baseline > 0.0 && candidate <= baseline * 0.9
    );
    metrics.candidate_successes >= metrics.baseline_successes && (time || tokens)
}

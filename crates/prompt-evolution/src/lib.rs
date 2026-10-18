//! Evidence-bound prompt evolution. Decisions are advisory: callers still enforce
//! publication, installation and repository gates. Holdout path validation must
//! be paired with sandbox filesystem permissions when starting an agent.
mod evaluation;
mod exploration;
mod holdout;
mod identity;
pub mod lineage;
mod metrics;
mod pairing;
mod protocol;
mod quarantine;
mod registry;

pub use contracts::{DatasetPartition, TrialVariant as Variant};
pub use evaluation::{evaluate_novel_capability, evaluate_promotion};
pub use exploration::{ExplorationBudget, ExplorationOutcome};
pub use holdout::HoldoutIsolation;
pub use protocol::{
    AblationStudy, EvaluationContext, EvaluationDecision, EvaluationMetrics, SnapshotBinding,
    TrialObservation,
};
pub use quarantine::QuarantinedRuleProposal;
pub use registry::{PromptDefinition, PromptRegistry, PromptVersion, StrategyPortfolio};

#[cfg(test)]
mod test_support;

//! File-based host adapters for prompt evolution. Gate inputs and receipts are
//! kept outside agent workspaces; candidate reports never supply approval flags.
//! These path checks do not replace OS-level filesystem isolation.

mod candidates;
mod dispatch;
mod evaluation;
mod exploration;
mod files;
mod lineage;
mod paths;
mod promotion;
mod protocol;
mod receipts;
mod selection;

pub use dispatch::{execute, register_prompt_candidate};
pub use lineage::{LineageReceipt, LineageRequest};
pub use protocol::{
    ExperimentReceipt, ExplorationUpdate, NoveltyRequest, PortfolioValidation, PromotionRequest,
    PromptRegistration, RuleProposalRequest, StrategyPromotionRequest,
};
pub use receipts::{EvaluationKind, EvaluationReceipt};
pub use selection::{
    SelectedStrategy, StrategySelections, resolve_registered_prompt, resolve_selection,
};

#[cfg(test)]
mod test_support;

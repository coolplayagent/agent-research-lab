//! Reviewed report publication with an explicitly configured repository layout and index adapter.
mod host;
mod index_refresh;
mod preparation;
mod protocol;
mod publication;
mod report;
mod snapshot;
mod validation;

pub use contracts::{FindingKind, SourceAccessStatus};
pub use index_refresh::refresh_index;
pub use preparation::prepare_report;
pub use protocol::{
    IndexRefreshRequest, IndexRefreshStatus, PrepareReportRequest, PreparedReport,
    PublicationLayout, PublishReportRequest,
};
pub use publication::publish_report;
pub use report::{
    EvidenceBindings, Finding, OutcomeReport, SourceReference, render_report, report_digest,
};
pub use validation::validate_report;

// Compatibility type name for earlier source consumers.
pub use OutcomeReport as ResearchReport;

#[cfg(test)]
mod test_support;

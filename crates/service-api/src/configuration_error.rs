//! Stable errors for service configuration mutations.
use contracts::ServiceConfigurationErrorCode;
#[derive(Debug)]
pub struct ConfigurationFailure(pub ServiceConfigurationErrorCode);
impl std::fmt::Display for ConfigurationFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}
impl std::error::Error for ConfigurationFailure {}

#[derive(Debug)]
pub struct LocalRuntimeFailure(pub contracts::LocalRuntimeErrorCode);
impl std::fmt::Display for LocalRuntimeFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}
impl std::error::Error for LocalRuntimeFailure {}

#[derive(serde::Serialize)]
pub struct LocalRuntimeStatus {
    pub component: contracts::LocalRuntimeComponent,
    pub state: contracts::LocalRuntimeState,
    pub packaged: bool,
    pub can_prepare: bool,
    pub error: Option<contracts::LocalRuntimeErrorCode>,
    pub health: Option<contracts::ServiceHealth>,
}

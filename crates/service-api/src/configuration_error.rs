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

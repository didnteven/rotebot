//! Shared data contracts. These describe intent; they never grant authority.

mod csv_mapping;
mod ids;
mod manifest;
mod task;

pub use csv_mapping::*;
pub use ids::*;
pub use manifest::*;
pub use task::*;

#[derive(Debug, thiserror::Error)]
pub enum DomainError {
    #[error("invalid contract: {0}")]
    InvalidContract(String),
}

/// Data-only request recorded by the sandbox. This confers no network authority.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlannedCall {
    pub connector_id: String,
    pub tool_name: String,
    pub args: serde_json::Value,
}

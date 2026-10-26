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

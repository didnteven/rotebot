//! Single-shot, zero-tool Meta Responses gateway.
mod images;
pub use images::*;
mod responses;
pub use responses::*;
mod custom;
pub use custom::*;

use macroscope_domain::{JobId, TaskId};
use serde::{Deserialize, Serialize};
use std::{future::Future, pin::Pin};

pub type GatewayFuture<'a> =
    Pin<Box<dyn Future<Output = Result<ValidatedOutput, GatewayError>> + Send + 'a>>;

/// A host-reviewed packet reference, not raw user data or a credential.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClearedPacket {
    pub packet_ref: String,
    pub lineage_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsentRef {
    pub epoch: u64,
    pub revision: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetReservation {
    pub reservation_ref: String,
    pub max_cost_microusd: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerateRequest {
    pub task_id: TaskId,
    pub job_id: JobId,
    pub packet: ClearedPacket,
    pub consent: ConsentRef,
    pub budget: BudgetReservation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidatedOutput {
    pub value: serde_json::Value,
    pub schema_ref: String,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum GatewayError {
    #[error("Skipped — monthly custom endpoint call cap reached")]
    CallCapReached,
    #[error("model gateway is unavailable")]
    Unavailable,
    #[error("credential rejected")]
    AuthRejected,
    #[error("billing required")]
    BillingRequired,
    #[error("quota exhausted")]
    Quota {
        retry_after: Option<std::time::Duration>,
    },
    #[error("payload too large")]
    PayloadTooLarge,
    #[error("temporary transport or provider failure")]
    Transient,
    #[error("output truncated")]
    Truncated,
    #[error("model refused the request")]
    Refused,
    #[error("output does not match the requested schema")]
    MalformedOutput,
    #[error("request rejected")]
    Rejected,
    #[error("model has no approved price")]
    UnsupportedModel,
    #[error("cost exceeds representable budget")]
    CostOverflow,
}

pub trait ModelGateway: Send + Sync {
    fn generate(&self, request: GenerateRequest) -> GatewayFuture<'_>;
}

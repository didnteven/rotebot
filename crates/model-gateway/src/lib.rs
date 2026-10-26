//! Single-shot model gateway seam. No transport or model access is implemented.

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

#[derive(Debug, thiserror::Error)]
pub enum GatewayError {
    #[error("model gateway is unavailable")]
    Unavailable,
}

pub trait ModelGateway: Send + Sync {
    fn generate(&self, request: GenerateRequest) -> GatewayFuture<'_>;
}

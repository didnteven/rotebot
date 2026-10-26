mod mapping;
pub use mapping::*;
// Provider-neutral coding sessions (plan §4.2), separate from single-shot generation.
// These declarations grant no tool, model, workspace, or credential access.

use futures_core::Stream;
use macroscope_domain::{BuildId, PermissionCeiling};
use serde::{Deserialize, Serialize};
use std::{future::Future, pin::Pin};

pub type BuildEvents = Pin<Box<dyn Stream<Item = Result<BuildEvent, BuildError>> + Send>>;
pub type AgentFuture<'a> = Pin<Box<dyn Future<Output = Result<(), BuildError>> + Send + 'a>>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildRef {
    pub id: BuildId,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildRequest {
    pub build: BuildRef,
    pub contract_hash: String,
    pub synthetic_workspace_handle: String,
    pub permission_ceiling: PermissionCeiling,
    pub model_route: ModelRoute,
    /// Opaque app-owned reference; never credential bytes.
    pub credential_ref: String,
    pub policy_epoch: u64,
    pub limits: BuildLimits,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelRoute {
    MetaApiKey,
    MusePlan,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildLimits {
    pub max_cost_microusd: u64,
    pub wall_seconds: u32,
    pub attempts: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BuildEvent {
    Phase { phase: String },
    Message { summary: String },
    ArtifactChanged { artifact_ref: String },
    Approval { request_ref: String },
    InputRequest { request_ref: String },
    Usage { cost_microusd: Option<u64> },
    Interrupted,
    CandidateCompleted { artifact_ref: String },
    Failed { code: String, summary: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScopedDecision {
    pub request_ref: String,
    pub policy_epoch: u64,
    pub allow: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum BuildError {
    #[error("coding agent is unavailable")]
    Unavailable,
    #[error("no model access: {0:?}")]
    NoModelAccess(NoAccessReason),
    #[error("build budget exhausted")]
    BudgetExceeded,
    #[error("egress consent revoked")]
    EgressDenied,
    #[error("candidate rejected by host validator")]
    Validation,
    #[error("synthetic samples do not match the contract")]
    InvalidSamples,
    #[error("credential unavailable")]
    Credential,
    #[error("Muse containment has not been verified")]
    ContainmentUnverified,
    #[error("synthetic workspace unavailable")]
    Workspace,
    #[error(transparent)]
    Gateway(#[from] macroscope_model_gateway::GatewayError),
    #[error(transparent)]
    Muse(#[from] macroscope_muse_client::MuseError),
}

pub trait CodingAgent: Send + Sync {
    fn start(&self, request: BuildRequest) -> BuildEvents;
    fn resume(&self, build: BuildRef) -> BuildEvents;
    fn answer(&self, build: BuildRef, decision: ScopedDecision) -> AgentFuture<'_>;
    fn cancel(&self, build: BuildRef) -> AgentFuture<'_>;
}

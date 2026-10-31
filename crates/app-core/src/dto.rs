//! Serde mirrors of `apps/desktop/src/backend/types.ts`. Field names and tagged-union
//! shapes must match that file exactly; `tests/dto_shapes.rs` pins the JSON.

use macroscope_domain::CsvMapping;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub version: String,
    pub platform: String,
    pub mock: bool,
    pub secret_store: macroscope_vault::SecretStoreKind,
    pub debug_build: bool,
}

// ---------- model access ----------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelRoute {
    ApiKey,
    CustomEndpoint,
    MusePlan,
    OfflineDemo,
}

impl ModelRoute {
    pub fn label(self) -> &'static str {
        match self {
            ModelRoute::CustomEndpoint => "Custom endpoint",
            ModelRoute::ApiKey => "Meta API key",
            ModelRoute::MusePlan => "Muse plan",
            ModelRoute::OfflineDemo => "Offline demo (no model)",
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            ModelRoute::CustomEndpoint => "custom_endpoint",
            ModelRoute::ApiKey => "api_key",
            ModelRoute::MusePlan => "muse_plan",
            ModelRoute::OfflineDemo => "offline_demo",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MuseState {
    Missing,
    UnsupportedVersion {
        version: String,
    },
    NotSignedIn {
        version: String,
    },
    SignedIn {
        version: String,
        #[serde(rename = "planSummary")]
        plan_summary: Option<String>,
    },
    Unknown {
        version: Option<String>,
        message: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EgressState {
    CloudAllowed,
    Stopping,
    LocalOnly,
    KeyUnavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum KeyTestResult {
    Ok {
        #[serde(rename = "checkedAtMs")]
        checked_at_ms: i64,
    },
    Rejected {
        #[serde(rename = "checkedAtMs")]
        checked_at_ms: i64,
    },
    BillingRequired {
        #[serde(rename = "checkedAtMs")]
        checked_at_ms: i64,
    },
    Quota {
        #[serde(rename = "checkedAtMs")]
        checked_at_ms: i64,
        #[serde(rename = "retryAtMs")]
        retry_at_ms: Option<i64>,
    },
    Network {
        #[serde(rename = "checkedAtMs")]
        checked_at_ms: i64,
        message: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiKeyStatus {
    pub present: bool,
    pub fingerprint: Option<String>,
    pub last_test: Option<KeyTestResult>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelAccessStatus {
    pub custom_endpoint: CustomEndpointStatus,
    pub api_key: ApiKeyStatus,
    pub muse: MuseState,
    pub selected_route: Option<ModelRoute>,
    pub effective_route: Option<ModelRoute>,
    pub blocked_reason: Option<String>,
    pub egress: EgressState,
    pub model: String,
    pub muse_unverified_ok: bool,
}

// ---------- files, drafts ----------

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TaskKind {
    #[default]
    CsvMapping,
    Program,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileHandle {
    pub handle_id: String,
    pub display_name: String,
    pub display_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CsvPreview {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    pub handle: FileHandle,
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub row_count: u64,
    pub byte_size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuestionOption {
    pub value: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleQuestion {
    pub id: String,
    pub column: String,
    pub prompt: String,
    pub options: Vec<QuestionOption>,
    pub answer: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskDraft {
    #[serde(default)]
    pub access_plan: Option<crate::api_discovery::AccessReport>,
    /// Explicit user selection, never inferred authority from a description.
    #[serde(default)]
    pub required_connections: Vec<SelectedConnection>,
    #[serde(default)]
    pub kind: TaskKind,
    #[serde(default)]
    pub example_output: Option<String>,
    #[serde(default)]
    pub synthetic_shape: String,
    /// True when the task reads approved web sources instead of a selected
    /// input file. `input` then carries a placeholder; `macroscope.input` is
    /// unavailable and the run plan hash covers fetched data instead.
    #[serde(default)]
    pub no_input: bool,
    pub draft_id: String,
    pub revision: u32,
    pub goal: String,
    pub input: CsvPreview,
    pub summary: Vec<String>,
    pub mapping: CsvMapping,
    pub questions: Vec<RuleQuestion>,
    pub mentioned_connectors: Vec<String>,
    pub output_name: String,
    pub ready_to_build: bool,
    pub blockers: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutboundSummaryItem {
    pub label: String,
    pub detail: Option<String>,
    pub real: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutboundSummary {
    pub recipient: String,
    pub model: String,
    pub includes: Vec<OutboundSummaryItem>,
    pub later_runs: Vec<String>,
    pub cost: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutboundPreview {
    pub summary: OutboundSummary,
    pub draft_id: String,
    pub route: ModelRoute,
    pub model: String,
    pub destination: String,
    pub payload_text: String,
    pub contains_real_rows: bool,
    pub estimated_cost_micros: Option<i64>,
    pub budget_micros: i64,
    /// Hosts a built web-source tool would fetch from. Known only after the
    /// model declares its sources, so this is empty before the build; the
    /// build-and-compare review lists the declared hosts for approval.
    #[serde(default)]
    pub web_hosts: Vec<String>,
}

// ---------- builds ----------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BuildPhase {
    Queued,
    Writing,
    Candidate,
    Checking,
    Ready,
    Failed,
    Stopped,
}

impl BuildPhase {
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            BuildPhase::Ready | BuildPhase::Failed | BuildPhase::Stopped
        )
    }
    pub fn as_str(self) -> &'static str {
        match self {
            BuildPhase::Queued => "queued",
            BuildPhase::Writing => "writing",
            BuildPhase::Candidate => "candidate",
            BuildPhase::Checking => "checking",
            BuildPhase::Ready => "ready",
            BuildPhase::Failed => "failed",
            BuildPhase::Stopped => "stopped",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckResult {
    pub id: String,
    pub label: String,
    pub passed: bool,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildEvent {
    pub build_id: String,
    pub seq: u32,
    pub phase: BuildPhase,
    pub message: String,
    pub at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Comparison {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_text: Option<String>,
    pub input_columns: Vec<String>,
    pub input_rows: Vec<Vec<String>>,
    pub output_columns: Vec<String>,
    pub output_rows: Vec<Vec<String>>,
    pub rows_in: u64,
    pub rows_out: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BuildFailureKind {
    Auth,
    Quota,
    Budget,
    Malformed,
    Checks,
    NoModelAccess,
    Internal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildFailure {
    pub kind: BuildFailureKind,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildState {
    #[serde(default)]
    pub program: Option<macroscope_builder::Program>,
    pub build_id: String,
    pub draft_id: String,
    pub route: ModelRoute,
    pub phase: BuildPhase,
    pub events: Vec<BuildEvent>,
    pub candidate: Option<CsvMapping>,
    pub checks: Vec<CheckResult>,
    pub comparison: Option<Comparison>,
    pub spent_micros: Option<i64>,
    pub failure: Option<BuildFailure>,
    pub route_label: String,
    /// What the test run's model calls cost, once it has made any. Filled when read.
    #[serde(default)]
    pub test_model_cost: Option<ModelCostSummary>,
}

// ---------- tools and runs ----------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolSummary {
    pub id: String,
    pub name: String,
    pub version: u32,
    pub engine: String,
    pub created_at_ms: i64,
    pub last_run_at_ms: Option<i64>,
    pub run_count: i64,
    pub sample: bool,
    pub state: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolDetail {
    #[serde(default = "default_model_cap")]
    pub model_step_cap: u32,
    #[serde(default)]
    pub program: Option<macroscope_builder::Program>,
    /// Model spend of the test run and the runs so far (micro-dollars). Absent without model calls.
    #[serde(default)]
    pub model_cost: Option<ModelCostSummary>,
    #[serde(flatten)]
    pub summary_row: ToolSummary,
    pub summary: Vec<String>,
    pub mapping: CsvMapping,
    pub input_columns: Vec<String>,
    pub output_columns: Vec<String>,
    pub access: Vec<String>,
    pub version_hash: String,
    /// True when the tool reads approved web sources instead of a selected
    /// input file. Runs then stage without an input handle.
    #[serde(default)]
    pub no_input: bool,
}

/// Amounts are micro-dollars of calls whose price was known; `*_unknown_calls` counts calls
/// the route did not price (shown as "price unknown", never as zero).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelCostSummary {
    pub test_micros: Option<i64>,
    pub test_calls: u32,
    pub test_unknown_calls: u32,
    pub last_run_micros: Option<i64>,
    pub last_run_calls: u32,
    pub last_run_unknown_calls: u32,
    pub run_count: u32,
    pub total_micros: i64,
    pub unknown_calls: u32,
}

/// An unverified suggestion from the public MCP registry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectorCandidate {
    pub name: String,
    pub description: String,
    pub url: String,
    /// Domain the registry name belongs to, for example "airtable.com".
    pub publisher: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StagedRun {
    pub creates_output: bool,
    #[serde(default)]
    pub planned_calls: Vec<macroscope_effect_broker::PlannedAction>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview_text: Option<String>,
    pub run_id: String,
    pub tool_id: String,
    pub input: FileHandle,
    pub output_dir: FileHandle,
    pub output_name: String,
    pub plan_hash: String,
    pub rows_in: u64,
    pub rows_out: u64,
    pub output_columns: Vec<String>,
    pub preview_rows: Vec<Vec<String>>,
    pub destination_exists: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RunFailure {
    InputInvalid {
        message: String,
        row: Option<u64>,
        column: Option<String>,
    },
    InputChanged {
        message: String,
    },
    DestinationExists {
        message: String,
    },
    DestinationChanged {
        message: String,
    },
    Revoked {
        message: String,
    },
    Internal {
        message: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FailureEnvelope<F> {
    pub failure: F,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum StageResult {
    Staged(Box<StagedRun>),
    Failed(FailureEnvelope<RunFailure>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CommitResult {
    Committed(Receipt),
    Failed(FailureEnvelope<RunFailure>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReceiptStatus {
    Committed,
    Reconciled,
    OutcomeUnknown,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Receipt {
    #[serde(default)]
    pub trigger: RunTrigger,
    #[serde(default)]
    pub schedule_id: Option<String>,
    #[serde(default)]
    pub actions: Vec<macroscope_effect_broker::ActionResult>,
    pub run_id: String,
    pub tool_id: String,
    pub tool_name: String,
    pub version_hash: String,
    pub plan_hash: String,
    pub input_digest: String,
    pub committed_digest: String,
    pub output_path: String,
    pub status: ReceiptStatus,
    pub at_ms: i64,
}

// ---------- activity and privacy ----------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityItem {
    pub id: String,
    pub at_ms: i64,
    pub kind: String,
    pub title: String,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutboundRecord {
    pub id: String,
    pub at_ms: i64,
    pub purpose: String,
    pub destination: String,
    pub route: ModelRoute,
    pub synthetic_only: bool,
    pub cost_micros: Option<i64>,
    pub payload_text: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredData {
    pub label: String,
    pub count: i64,
    pub bytes: i64,
    pub expires: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrivacyOverview {
    pub egress: EgressState,
    pub stored: Vec<StoredData>,
    pub outbound: Vec<OutboundRecord>,
}

// ---------- connection suggestions and Show (ADR 0001 §2, plan §5) ----------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionSuggestion {
    pub connector_id: String,
    pub connector_name: String,
    pub message: String,
    /// "foreground_app" | "window_title" | "browser_host" | "task_text"
    pub signal_kind: String,
    pub count: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ShowState {
    Unavailable {
        reason: String,
    },
    Idle {
        /// "granted" | "denied" | "unknown"
        accessibility: String,
    },
    Recording {
        #[serde(rename = "sessionId")]
        session_id: String,
        #[serde(rename = "startedAtMs")]
        started_at_ms: i64,
        #[serde(rename = "maxSeconds")]
        max_seconds: u32,
        paused: bool,
        #[serde(rename = "appsSeen")]
        apps_seen: u32,
        matches: u32,
    },
    Finished {
        #[serde(rename = "sessionId")]
        session_id: String,
        #[serde(rename = "durationSeconds")]
        duration_seconds: u32,
        #[serde(rename = "appsSeen")]
        apps_seen: Vec<String>,
        suggestions: Vec<ConnectionSuggestion>,
    },
}

// ---------- connections (ADR 0001 §2) ----------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ConnectionState {
    NotConnected,
    Connecting,
    Connected {
        #[serde(rename = "sinceMs")]
        since_ms: i64,
        #[serde(rename = "toolCount")]
        tool_count: u32,
    },
    NeedsReauth,
    Error {
        message: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectorInfo {
    pub id: String,
    pub name: String,
    pub category: String,
    pub mcp_url: String,
    /// "dynamic" | "preregistered"
    pub registration: String,
    /// "verified-docs" | "verified-live" | "unverified"
    pub verification: String,
    pub docs_url: Option<String>,
    pub state: ConnectionState,
    pub has_client_credentials: bool,
    pub client_setup: Option<String>,
    pub redirect_uri: Option<String>,
    /// Connections to the same service share this name (see the catalog).
    #[serde(default)]
    pub service: String,
    /// Set for email connectors that sign in with an address and an app password.
    #[serde(default)]
    pub mail: Option<MailInfo>,
}

/// Server names a person types for a custom mail provider.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MailServerInput {
    pub imap_host: String,
    pub imap_port: u16,
    pub smtp_host: String,
    pub smtp_port: u16,
    #[serde(default)]
    pub smtp_starttls: bool,
}

/// Mail server defaults and, once connected, the saved address. Never the password.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MailInfo {
    pub help: String,
    pub custom_server: bool,
    pub imap_host: String,
    pub imap_port: u16,
    pub smtp_host: String,
    pub smtp_port: u16,
    pub smtp_starttls: bool,
    pub email: Option<String>,
    /// The provider's page for creating an app password.
    #[serde(default)]
    pub app_password_url: Option<String>,
}


#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectorTool {
    #[serde(default)]
    pub input_schema: serde_json::Value,
    pub name: String,
    pub description: String,
    pub read_only: Option<bool>,
    pub destructive: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectedConnection {
    pub connector_id: String,
    pub name: String,
    pub resource: String,
    pub tools: Vec<ConnectorTool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectStart {
    pub connector_id: String,
    pub authorization_url: String,
    pub pending_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteItem {
    pub label: String,
    pub count: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeletePreview {
    pub items: Vec<DeleteItem>,
    pub tools_included: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeleteOptions {
    pub include_tools: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunTrigger {
    #[default]
    Manual,
    Schedule,
}
impl RunFailure {
    pub fn message(&self) -> String {
        match self {
            Self::InputInvalid { message, .. }
            | Self::InputChanged { message }
            | Self::DestinationExists { message }
            | Self::DestinationChanged { message }
            | Self::Revoked { message }
            | Self::Internal { message } => message.clone(),
        }
    }
}

fn default_model_cap() -> u32 {
    60
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomEndpointStatus {
    pub base_url: Option<String>,
    pub host: Option<String>,
    pub model: Option<String>,
    pub display_name: Option<String>,
    pub token_present: bool,
    pub last_test: Option<CustomEndpointTest>,
    pub monthly_call_cap: u32,
    pub calls_this_month: u64,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomEndpointTest {
    pub status: String,
    pub latency_ms: u64,
}

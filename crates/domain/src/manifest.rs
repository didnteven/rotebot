use crate::{BuildId, ConnectorId, ToolId};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ToolManifest {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "model_step"
    )]
    pub model_step: Option<ModelStep>,
    pub schema: ToolSchema,
    pub id: ToolId,
    pub name: String,
    pub version: u32,
    pub runtime: Runtime,
    pub platforms: Vec<Platform>,
    /// JSON Schema describing host-selected input handles, never raw paths.
    pub inputs: serde_json::Value,
    pub triggers: Vec<Trigger>,
    pub capabilities: Capabilities,
    pub limits: Limits,
    pub execution: Execution,
    pub tests: TestSuite,
    pub provenance: Provenance,
    pub integrity: Integrity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum ToolSchema {
    #[serde(rename = "macroscope.tool/v1")]
    V1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Runtime {
    pub engine: RuntimeEngine,
    pub pin: String,
    pub sdk: String,
    pub entry: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum RuntimeEngine {
    Deno,
    CsvMapping,
    JsQuickjs,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    Macos,
    Windows,
    Linux,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Trigger {
    Manual {},
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Capabilities {
    pub file_read: Vec<FileRead>,
    pub file_create: Vec<FileCreate>,
    pub file_move: Vec<String>,
    pub file_trash: Vec<String>,
    pub network: Vec<String>,
    pub clipboard: Clipboard,
    pub native_apps: Vec<String>,
    pub secrets: Vec<String>,
    pub model_calls: bool,
    #[serde(default)]
    pub mcp: Vec<McpCapability>,
}

/// A connection declaration is not a grant. Each call still needs broker approval.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct McpCapability {
    pub connector_id: ConnectorId,
    pub tool_names: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FileRead {
    pub handle: String,
    pub max_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FileCreate {
    pub root: String,
    pub name_pattern: String,
    pub overwrite: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Clipboard {
    #[default]
    None,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Limits {
    pub wall_seconds: u32,
    #[serde(rename = "memoryMiB")]
    pub memory_mib: u32,
    pub effects: u32,
    pub output_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Execution {
    pub initial_trust: InitialTrust,
    pub concurrency: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum InitialTrust {
    Supervised,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TestSuite {
    pub suite_ref: String,
    pub fixture_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Provenance {
    pub contract_hash: String,
    pub build_ref: BuildId,
    pub model: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Integrity {
    pub tree_hash: String,
    pub approval_ref: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ModelStep {
    pub purpose: String,
}

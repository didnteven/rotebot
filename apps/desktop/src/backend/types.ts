// Product API between the React UI and the Rust core (plan §3.1 typed Backend seam).
// The Tauri implementation calls #[tauri::command]s with the same names in snake_case;
// the mock implementation keeps the same state machine in memory for browser tests.
// Every string that came from a file, a model, or an MCP server is untrusted: render it
// as text, never as HTML (plan §8.4).

export interface AppInfo {
  version: string;
  platform: "macos" | "windows" | "linux" | "browser";
  /** True when running against the in-memory mock (browser dev / tests). */
  mock: boolean;
}

// ---------- Model access (ADR 0001 §1) ----------

export type ModelRoute = "api_key" | "muse_plan" | "offline_demo";

export type MuseState =
  | { kind: "missing" }
  | { kind: "unsupported_version"; version: string }
  | { kind: "not_signed_in"; version: string }
  | { kind: "signed_in"; version: string; planSummary: string | null }
  | { kind: "unknown"; version: string | null; message: string };

export type EgressState = "cloud_allowed" | "stopping" | "local_only" | "key_unavailable";

export interface ModelAccessStatus {
  apiKey: { present: boolean; fingerprint: string | null; lastTest: KeyTestResult | null };
  muse: MuseState;
  /** The route the user picked; null until they choose. */
  selectedRoute: ModelRoute | null;
  /** The route a build would use right now, after availability checks. */
  effectiveRoute: ModelRoute | null;
  /** Why no cloud route is usable, in plain words, when effectiveRoute is offline/null. */
  blockedReason: string | null;
  egress: EgressState;
  model: string; // "muse-spark-1.3"
}

export type KeyTestResult =
  | { kind: "ok"; checkedAtMs: number }
  | { kind: "rejected"; checkedAtMs: number }
  | { kind: "billing_required"; checkedAtMs: number }
  | { kind: "quota"; checkedAtMs: number; retryAtMs: number | null }
  | { kind: "network"; checkedAtMs: number; message: string };

// ---------- Files, samples, contracts ----------

/** A file the user chose with the native picker. The path is shown, never editable. */
export interface FileHandle {
  handleId: string;
  displayName: string;
  displayPath: string;
}

export interface CsvPreview {
  handle: FileHandle;
  columns: string[];
  rows: string[][]; // first rows only (≤ 8), cells truncated to 200 chars
  rowCount: number;
  byteSize: number;
}

export type DateFormat = "mm_dd_yy" | "mm_dd_yyyy" | "dd_mm_yyyy" | "yyyy_mm_dd" | "iso8601";

export type Transform =
  | { type: "copy" }
  | { type: "trim" }
  | { type: "upper" }
  | { type: "lower" }
  | { type: "date"; from: DateFormat; to: DateFormat }
  | { type: "money"; currency: string; decimals: number }
  | { type: "constant"; value: string };

export interface OutputColumn {
  name: string;
  source: string;
  transform: Transform;
}

export interface CsvMapping {
  output_columns: OutputColumn[];
  on_error: "stop_and_explain";
  skip_header_rows: number;
  formula_escape: boolean;
}

/** A question the host asks before building (plan §2.2: "Does 03/04/26 mean March 4 or April 3?"). */
export interface RuleQuestion {
  id: string;
  column: string;
  prompt: string;
  options: { value: string; label: string }[];
  answer: string | null;
}

export interface TaskDraft {
  draftId: string;
  revision: number;
  goal: string;
  input: CsvPreview;
  /** Plain-language summary lines shown as the "Task summary" (plan §2.1 wording). */
  summary: string[];
  mapping: CsvMapping;
  questions: RuleQuestion[];
  /** Connector ids the description mentions (ADR 0001 §2), e.g. ["gmail"]. */
  mentionedConnectors: string[];
  outputName: string; // e.g. "orders_ready.csv"
  readyToBuild: boolean;
  /** Why it is not ready (unanswered question, missing column...). */
  blockers: string[];
}

export interface OutboundPreview {
  draftId: string;
  route: ModelRoute;
  model: string;
  destination: string; // "Meta API (api.meta.ai)" | "Muse Code with your Muse plan" | "Nothing leaves this Mac"
  /** Exact text that will be sent (direct route) or supplied to Muse (muse route). Synthetic rows only. */
  payloadText: string;
  containsRealRows: false;
  estimatedCostMicros: number | null; // null for plan route (counts against the plan allowance)
  budgetMicros: number;
}

// ---------- Builds ----------

export type BuildPhase =
  | "queued"
  | "writing"      // model is producing a candidate
  | "candidate"    // "Candidate received" — not ready
  | "checking"     // host checks running
  | "ready"        // host checks passed; staged result available to compare
  | "failed"
  | "stopped";

export interface CheckResult {
  id: string;
  label: string; // "Row count preserved", "Dates converted", "Source unchanged", "Malformed date stops"
  passed: boolean;
  detail: string | null;
}

export interface BuildEvent {
  buildId: string;
  seq: number;
  phase: BuildPhase;
  message: string;
  atMs: number;
}

export interface BuildState {
  buildId: string;
  draftId: string;
  route: ModelRoute;
  phase: BuildPhase;
  events: BuildEvent[];
  candidate: CsvMapping | null;
  checks: CheckResult[];
  /** Host-rendered comparison of the staged sample output (never HTML). */
  comparison: { inputColumns: string[]; inputRows: string[][]; outputColumns: string[]; outputRows: string[][]; rowsIn: number; rowsOut: number } | null;
  spentMicros: number | null;
  failure: { kind: "auth" | "quota" | "budget" | "malformed" | "checks" | "no_model_access" | "internal"; message: string } | null;
  /** Route label for the UI, e.g. "Offline demo (no model)". */
  routeLabel: string;
}

// ---------- Tools and runs ----------

export interface ToolSummary {
  id: string;
  name: string;
  version: number;
  engine: "csv-mapping";
  createdAtMs: number;
  lastRunAtMs: number | null;
  runCount: number;
  sample: boolean;
}

export interface ToolDetail extends ToolSummary {
  summary: string[];
  mapping: CsvMapping;
  inputColumns: string[];
  outputColumns: string[];
  access: string[]; // "Read the CSV you choose", "Create one new CSV in the folder you choose", "Never overwrites"
  versionHash: string;
}

export interface StagedRun {
  runId: string;
  toolId: string;
  input: FileHandle;
  outputDir: FileHandle;
  outputName: string;
  planHash: string;
  rowsIn: number;
  rowsOut: number;
  outputColumns: string[];
  previewRows: string[][];
  destinationExists: boolean;
}

export type RunFailure =
  | { kind: "input_invalid"; message: string; row: number | null; column: string | null }
  | { kind: "input_changed"; message: string }
  | { kind: "destination_exists"; message: string }
  | { kind: "destination_changed"; message: string }
  | { kind: "revoked"; message: string }
  | { kind: "internal"; message: string };

export interface Receipt {
  runId: string;
  toolId: string;
  toolName: string;
  versionHash: string;
  planHash: string;
  inputDigest: string;
  committedDigest: string;
  outputPath: string;
  status: "committed" | "reconciled" | "outcome_unknown";
  atMs: number;
}

// ---------- Activity and privacy ----------

export interface ActivityItem {
  id: string;
  atMs: number;
  kind: "build" | "run" | "outbound" | "connection" | "show" | "settings";
  title: string;
  detail: string | null;
}

export interface OutboundRecord {
  id: string;
  atMs: number;
  purpose: string;
  destination: string;
  route: ModelRoute;
  syntheticOnly: boolean;
  costMicros: number | null;
  payloadText: string | null; // kept 7 days (plan §8.3)
}

export interface PrivacyOverview {
  egress: EgressState;
  stored: { label: string; count: number; bytes: number; expires: string }[];
  outbound: OutboundRecord[];
}

// ---------- Connections (ADR 0001 §2) ----------

export type ConnectionState =
  | { kind: "not_connected" }
  | { kind: "connecting" }
  | { kind: "connected"; sinceMs: number; toolCount: number }
  | { kind: "needs_reauth" }
  | { kind: "error"; message: string };

export interface ConnectorInfo {
  id: string; // "gmail"
  name: string; // "Gmail"
  category: string;
  mcpUrl: string;
  registration: "dynamic" | "preregistered";
  verification: "verified-docs" | "unverified";
  docsUrl: string | null;
  state: ConnectionState;
  /** For preregistered servers: whether the user has stored a client id. */
  hasClientCredentials: boolean;
}

export interface ConnectionSuggestion {
  connectorId: string;
  connectorName: string;
  message: string; // "You used Gmail while recording. Connect Gmail so tools can work with it."
  signalKind: "foreground_app" | "window_title" | "browser_host" | "task_text";
  count: number;
}

export interface ConnectStart {
  connectorId: string;
  authorizationUrl: string; // opened in the system browser by the core, shown for copying
  pendingId: string;
}

// ---------- Show (gated; plan §5, ADR 0001 §2) ----------

export type ShowState =
  | { kind: "unavailable"; reason: string }
  | { kind: "idle"; accessibility: "granted" | "denied" | "unknown" }
  | { kind: "recording"; sessionId: string; startedAtMs: number; maxSeconds: number; paused: boolean; appsSeen: number; matches: number }
  | { kind: "finished"; sessionId: string; durationSeconds: number; appsSeen: string[]; suggestions: ConnectionSuggestion[] };

// ---------- The seam ----------

export interface Backend {
  appInfo(): Promise<AppInfo>;

  // Model access
  modelAccess(): Promise<ModelAccessStatus>;
  /** macOS: opens a native hidden-input dialog in the core; the key never enters the webview. */
  enterApiKeyNative(): Promise<ModelAccessStatus>;
  /** Fallback for platforms without the native dialog; the UI clears its field immediately. */
  saveApiKey(key: string): Promise<ModelAccessStatus>;
  removeApiKey(): Promise<ModelAccessStatus>;
  testApiKey(): Promise<ModelAccessStatus>;
  refreshMuseStatus(): Promise<ModelAccessStatus>;
  /** Opens Terminal running `muse login` — only on an explicit user click. */
  openMuseSignIn(): Promise<void>;
  selectRoute(route: ModelRoute): Promise<ModelAccessStatus>;
  setLocalOnly(localOnly: boolean): Promise<ModelAccessStatus>;

  // Create (Tell)
  pickCsv(): Promise<CsvPreview | null>; // null when the user cancels
  loadSampleCsv(): Promise<CsvPreview>;
  draftTask(input: FileHandle, description: string): Promise<TaskDraft>;
  answerQuestion(draftId: string, questionId: string, answer: string): Promise<TaskDraft>;
  updateMapping(draftId: string, mapping: CsvMapping, outputName: string): Promise<TaskDraft>;
  outboundPreview(draftId: string): Promise<OutboundPreview>;
  startBuild(draftId: string, consent: { outboundReviewed: boolean }): Promise<BuildState>;
  buildState(buildId: string): Promise<BuildState>;
  /** Subscribe to build events; returns an unsubscribe function. */
  onBuildEvent(buildId: string, cb: (state: BuildState) => void): Promise<() => void>;
  stopBuild(buildId: string): Promise<BuildState>;
  saveTool(buildId: string, name: string): Promise<ToolDetail>;

  // Tools and runs
  listTools(): Promise<ToolSummary[]>;
  getTool(toolId: string): Promise<ToolDetail>;
  installSampleTool(): Promise<ToolDetail>;
  deleteTool(toolId: string): Promise<void>;
  pickOutputDir(): Promise<FileHandle | null>;
  stageRun(toolId: string, input: FileHandle, outputDir: FileHandle, outputName: string): Promise<StagedRun | { failure: RunFailure }>;
  commitRun(runId: string, planHash: string): Promise<Receipt | { failure: RunFailure }>;
  discardRun(runId: string): Promise<void>;
  revealOutput(receiptRunId: string): Promise<void>;

  // Activity and privacy
  listActivity(): Promise<ActivityItem[]>;
  listReceipts(): Promise<Receipt[]>;
  privacyOverview(): Promise<PrivacyOverview>;
  deleteOutboundHistory(): Promise<void>;
  emergencyStop(): Promise<void>;

  // Connections
  listConnectors(): Promise<ConnectorInfo[]>;
  setConnectorUrl(connectorId: string, url: string): Promise<ConnectorInfo>;
  setClientCredentials(connectorId: string, clientId: string, clientSecret: string | null): Promise<ConnectorInfo>;
  beginConnect(connectorId: string): Promise<ConnectStart>;
  /** Resolves when the browser redirect completes (or fails/times out). */
  completeConnect(pendingId: string): Promise<ConnectorInfo>;
  testConnection(connectorId: string): Promise<ConnectorInfo>;
  disconnect(connectorId: string): Promise<ConnectorInfo>;
  connectionSuggestions(): Promise<ConnectionSuggestion[]>;
  dismissSuggestion(connectorId: string): Promise<void>;

  // Show
  showState(): Promise<ShowState>;
  startShow(): Promise<ShowState>;
  pauseShow(paused: boolean): Promise<ShowState>;
  stopShow(): Promise<ShowState>;
  onShowEvent(cb: (state: ShowState) => void): Promise<() => void>;
}

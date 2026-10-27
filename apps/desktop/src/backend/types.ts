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
  secretStore: "file" | "keychain" | "memory" | "credential_manager";
  debugBuild: boolean;
}

// ---------- Model access (ADR 0001 §1) ----------

export type ModelRoute = "api_key" | "muse_plan" | "offline_demo";

export type MuseState =
  | { kind: "missing" }
  | { kind: "unsupported_version"; version: string }
  | { kind: "not_signed_in"; version: string }
  | { kind: "signed_in"; version: string; planSummary: string | null }
  | { kind: "unknown"; version: string | null; message: string };

export type EgressState =
  "cloud_allowed" | "stopping" | "local_only" | "key_unavailable";

export interface ModelAccessStatus {
  apiKey: {
    present: boolean;
    fingerprint: string | null;
    lastTest: KeyTestResult | null;
  };
  muse: MuseState;
  /** The route the user picked; null until they choose. */
  selectedRoute: ModelRoute | null;
  /** The route a build would use right now, after availability checks. */
  effectiveRoute: ModelRoute | null;
  /** Why no cloud route is usable, in plain words. Informational when effectiveRoute is
   *  "offline_demo" (building still works without a model); blocking only when effectiveRoute is null. */
  blockedReason: string | null;
  egress: EgressState;
  model: string; // "muse-spark-1.3"
  /** Owner opt-in to Muse plan builds before Muse's file-reading tools are proven confined
   *  to the build folder (plan §4.3 containment gate). Shell and writes stay disabled. */
  museUnverifiedOk: boolean;
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
  text?: string;
  handle: FileHandle;
  columns: string[];
  rows: string[][]; // first rows only (≤ 8), cells truncated to 200 chars
  rowCount: number;
  byteSize: number;
}

export type DateFormat =
  | "mm_dd_yy"
  | "mm_dd_yyyy"
  | "dd_mm_yy"
  | "dd_mm_yyyy"
  | "yyyy_mm_dd"
  | "iso8601";

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

export type TaskKind = "csv-mapping" | "program";
/** A web source the host fetches and passes to the program as data (ADR 0004). */
export interface WebSource { name: string; url: string; }
export interface Program { summary: string; output_name: string; code: string; webSources: WebSource[]; }
export interface TaskDraft {
  requiredConnections?: SelectedConnection[];
  kind?: TaskKind;
  exampleOutput?: string | null;
  syntheticShape?: string;
  /** True when the task reads approved web sources instead of a selected file. */
  noInput: boolean;
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
  /** Exact text that will be sent (direct route) or supplied to Muse (muse route). Synthetic shape and explicitly reviewed user example only. */
  payloadText: string;
  containsRealRows: boolean;
  estimatedCostMicros: number | null; // null for plan route (counts against the plan allowance)
  budgetMicros: number;
  /** Hosts a built web-source tool would fetch from (empty before the build). */
  webHosts: string[];
}

// ---------- Builds ----------

export type BuildPhase =
  | "queued"
  | "writing" // model is producing a candidate
  | "candidate" // "Candidate received" — not ready
  | "checking" // host checks running
  | "ready" // host checks passed; staged result available to compare
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
  program?: Program | null;
  buildId: string;
  draftId: string;
  route: ModelRoute;
  phase: BuildPhase;
  events: BuildEvent[];
  candidate: CsvMapping | null;
  checks: CheckResult[];
  /** Host-rendered comparison of the staged sample output (never HTML). */
  comparison: {
    inputText?: string;
    outputText?: string;
    inputColumns: string[];
    inputRows: string[][];
    outputColumns: string[];
    outputRows: string[][];
    rowsIn: number;
    rowsOut: number;
  } | null;
  spentMicros: number | null;
  failure: {
    kind:
      | "auth"
      | "quota"
      | "budget"
      | "malformed"
      | "checks"
      | "no_model_access"
      | "internal";
    message: string;
  } | null;
  /** Route label for the UI, e.g. "Offline demo (no model)". */
  routeLabel: string;
}

// ---------- Tools and runs ----------

export interface ToolSummary {
  id: string;
  name: string;
  version: number;
  engine: "csv-mapping" | "js-quickjs";
  createdAtMs: number;
  lastRunAtMs: number | null;
  runCount: number;
  sample: boolean;
  state: "enabled" | "paused";
}

export interface ToolDetail extends ToolSummary {
  program?: Program | null;
  summary: string[];
  mapping: CsvMapping;
  inputColumns: string[];
  outputColumns: string[];
  access: string[]; // "Read the CSV you choose", "Create one new CSV in the folder you choose", "Never overwrites"
  versionHash: string;
  /** True when the tool reads approved web sources instead of a selected file. */
  noInput: boolean;
}

export interface PlannedCall { connectorId: string; toolName: string; args: Record<string, unknown>; }
export interface PlannedAction { call: PlannedCall; service: string; resource: string; readOnlyHint: boolean | null; destructiveHint: boolean | null; }
export interface ActionResult { action: PlannedAction; status: "succeeded" | "failed" | "outcome_unknown" | "not_run"; summary: string; }
export interface StagedRun {
  createsOutput?: boolean;
  plannedCalls?: PlannedAction[];
  previewText?: string;
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
  | {
      kind: "input_invalid";
      message: string;
      row: number | null;
      column: string | null;
    }
  | { kind: "input_changed"; message: string }
  | { kind: "destination_exists"; message: string }
  | { kind: "destination_changed"; message: string }
  | { kind: "revoked"; message: string }
  | { kind: "internal"; message: string };

export interface Receipt {
  trigger?: "manual" | "schedule";
  scheduleId?: string | null;
  actions?: ActionResult[];
  runId: string;
  toolId: string;
  toolName: string;
  versionHash: string;
  planHash: string;
  inputDigest: string;
  committedDigest: string;
  outputPath: string;
  status: "committed" | "reconciled" | "outcome_unknown" | "failed";
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

export interface DeletePreview {
  items: { label: string; count: number }[];
  toolsIncluded: false;
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
  verification: "verified-docs" | "verified-live" | "unverified";
  /** For preregistered servers: one or two sentences on creating an OAuth client. */
  clientSetup: string | null;
  /** Loopback redirect the user must register for preregistered clients (fixed port), if any. */
  redirectUri: string | null;
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

export interface SelectedConnection { connectorId: string; name: string; resource: string; tools: ConnectorTool[]; }
export interface ConnectorTool {
  inputSchema?: unknown;
  name: string;
  description: string; // untrusted server text, truncated; render as text
  readOnly: boolean | null;
  destructive: boolean | null;
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
  | {
      kind: "recording";
      sessionId: string;
      startedAtMs: number;
      maxSeconds: number;
      paused: boolean;
      appsSeen: number;
      matches: number;
    }
  | {
      kind: "finished";
      sessionId: string;
      durationSeconds: number;
      appsSeen: string[];
      suggestions: ConnectionSuggestion[];
    };

// ---------- The seam ----------

export type Frequency = {kind: "daily"; time: string} | {kind: "weekly"; weekday: number; time: string} | {kind: "monthly"; day: number; time: string};
export interface ScheduleOutputs { today: boolean; notification: boolean; folder: FileHandle | null; filename: string }
export interface ScheduleRequest { frequency: Frequency; outputs: ScheduleOutputs; versionHash: string; approved: boolean }
export interface Schedule { id: string; toolId: string; frequency: Frequency; outputs: ScheduleOutputs; versionHash: string; binding: string; folderIdentity: string | null; status: "enabled" | "paused" | "needs_reapproval" | "emergency_paused"; approvedAtMs: number; lastDueMs: number; nextRunMs: number | null }
export interface TodayEntry { id: string; toolId: string; toolName: string; trigger: "schedule" | "manual"; scheduleId: string | null; startedAtMs: number; finishedAtMs: number | null; status: "done" | "needs_you" | "skipped" | "failed" | "staged"; body: string; note: string | null; fileCreated: string | null; runId: string | null; staged: StagedRun | null; notification: boolean; addToToday: boolean; dismissed: boolean }
export interface TodayDay { day: string; entries: TodayEntry[] }

export interface Backend {
  listToday(): Promise<TodayDay[]>;
  dismissTodayEntry(id: string): Promise<void>;
  listSchedules(): Promise<Schedule[]>;
  setSchedule(toolId: string, request: ScheduleRequest): Promise<Schedule>;
  scheduleStatus(toolId: string, status: "enabled" | "paused" | "remove"): Promise<void>;
  loginEnabled(): Promise<boolean>;
  setLoginEnabled(enabled: boolean): Promise<void>;
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
  setMuseUnverifiedOk(ok: boolean): Promise<ModelAccessStatus>;
  setLocalOnly(localOnly: boolean): Promise<ModelAccessStatus>;

  // Create (Tell)
  pickCsv(): Promise<CsvPreview | null>; // null when the user cancels
  loadSampleCsv(): Promise<CsvPreview>;
  draftTask(input: FileHandle, description: string): Promise<TaskDraft>;
  /** Drafts a program task with no input file; the program reads approved web sources. */
  draftTaskNoInput(description: string): Promise<TaskDraft>;
  answerQuestion(
    draftId: string,
    questionId: string,
    answer: string,
  ): Promise<TaskDraft>;
  selectTaskConnections(draftId: string, connectorIds: string[]): Promise<TaskDraft>;
  updateTask(draftId: string, kind: TaskKind, outputName: string, exampleOutput: string | null): Promise<TaskDraft>;
  updateMapping(
    draftId: string,
    mapping: CsvMapping,
    outputName: string,
  ): Promise<TaskDraft>;
  outboundPreview(draftId: string): Promise<OutboundPreview>;
  startBuild(
    draftId: string,
    consent: { outboundReviewed: boolean },
  ): Promise<BuildState>;
  buildState(buildId: string): Promise<BuildState>;
  /** Subscribe to build events; returns an unsubscribe function. */
  onBuildEvent(
    buildId: string,
    cb: (state: BuildState) => void,
  ): Promise<() => void>;
  buildTryInput(buildId: string, input: FileHandle): Promise<BuildState>;
  stopBuild(buildId: string): Promise<BuildState>;
  saveTool(buildId: string, name: string): Promise<ToolDetail>;

  // Tools and runs
  listTools(): Promise<ToolSummary[]>;
  getTool(toolId: string): Promise<ToolDetail>;
  installSampleTool(): Promise<ToolDetail>;
  exportTool(toolId: string): Promise<string | null>; // null when the folder picker is canceled
  revokeTool(toolId: string): Promise<ToolDetail>;
  restoreTool(toolId: string): Promise<ToolDetail>;
  deleteTool(toolId: string): Promise<void>;
  pickOutputDir(): Promise<FileHandle | null>;
  stageRun(
    toolId: string,
    input: FileHandle,
    outputDir: FileHandle,
    outputName: string,
  ): Promise<StagedRun | { failure: RunFailure }>;
  /** Stages a run for a tool with no input file; the host fetches the approved web sources. */
  stageRunNoInput(
    toolId: string,
    outputDir: FileHandle,
    outputName: string,
  ): Promise<StagedRun | { failure: RunFailure }>;
  commitRun(
    runId: string,
    planHash: string,
    actionsReviewed?: boolean,
  ): Promise<Receipt | { failure: RunFailure }>;
  discardRun(runId: string): Promise<void>;
  revealOutput(receiptRunId: string): Promise<void>;

  // Activity and privacy
  listActivity(): Promise<ActivityItem[]>;
  listReceipts(): Promise<Receipt[]>;
  privacyOverview(): Promise<PrivacyOverview>;
  deletePreview(): Promise<DeletePreview>;
  deleteData(options: { includeTools: boolean }): Promise<void>;
  deleteOutboundHistory(): Promise<void>;
  emergencyStop(): Promise<void>;
  /** Opens a documentation or authorization URL in the system browser. The core only allows
   *  https URLs from the connector catalog, Muse/Meta docs, or a pending authorization. */
  openExternal(url: string): Promise<void>;

  // Connections
  listConnectors(): Promise<ConnectorInfo[]>;
  setConnectorUrl(connectorId: string, url: string): Promise<ConnectorInfo>;
  setClientCredentials(
    connectorId: string,
    clientId: string,
    clientSecret: string | null,
  ): Promise<ConnectorInfo>;
  beginConnect(connectorId: string): Promise<ConnectStart>;
  /** Resolves when the browser redirect completes (or fails/times out). */
  completeConnect(pendingId: string): Promise<ConnectorInfo>;
  testConnection(connectorId: string): Promise<ConnectorInfo>;
  disconnect(connectorId: string): Promise<ConnectorInfo>;
  /** Tools a server lists without signing in (some servers, e.g. Gmail, allow this). Empty if it requires auth. */
  previewConnectorTools(connectorId: string): Promise<ConnectorTool[]>;
  connectionSuggestions(): Promise<ConnectionSuggestion[]>;
  dismissSuggestion(connectorId: string): Promise<void>;

  // Show
  showState(): Promise<ShowState>;
  startShow(): Promise<ShowState>;
  pauseShow(paused: boolean): Promise<ShowState>;
  stopShow(): Promise<ShowState>;
  onShowEvent(cb: (state: ShowState) => void): Promise<() => void>;
}

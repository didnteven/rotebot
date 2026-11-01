// Product API between the React UI and the Rust core (plan §3.1 typed Backend seam).
// The Tauri implementation calls #[tauri::command]s with the same names in snake_case;
// the mock implementation keeps the same state machine in memory for browser tests.
// Every string that came from a file, a model, or an MCP server is untrusted: render it
// as text, never as HTML (plan §8.4).

export interface BotDesign {
  id: string;
  description: string;
  completedPoses: number[];
  attempts: number;
  uncertainPose: number | null;
  transparentPoses?: number[];
  backgroundRemovalError?: string | null;
  createdAtMs: number;
  images: Record<string, string>;
}
export interface BotImageRequest {
  id: string; description: string; pose: number; approved: boolean; retryUncertain: boolean;
}

export interface AppInfo {
  version: string;
  platform: "macos" | "windows" | "linux" | "browser";
  /** True when running against the in-memory mock (browser dev / tests). */
  mock: boolean;
  secretStore: "file" | "keychain" | "memory" | "credential_manager";
  debugBuild: boolean;
}

// ---------- Model access (ADR 0001 §1) ----------

export type ModelRoute = "api_key" | "muse_plan" | "custom_endpoint" | "offline_demo";

export type MuseState =
  | { kind: "missing" }
  | { kind: "unsupported_version"; version: string }
  | { kind: "not_signed_in"; version: string }
  | { kind: "signed_in"; version: string; planSummary: string | null }
  | { kind: "unknown"; version: string | null; message: string };

export type EgressState =
  "cloud_allowed" | "stopping" | "local_only" | "key_unavailable";

export interface CustomEndpointStatus {
  baseUrl: string | null;
  host: string | null;
  model: string | null;
  displayName: string | null;
  tokenPresent: boolean;
  lastTest: {status: string; latencyMs: number} | null;
  monthlyCallCap: number;
  callsThisMonth: number;
}
export interface ModelAccessStatus {
  customEndpoint: CustomEndpointStatus;
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
export interface ApiConfig { name: string; url: string; auth: "none" | "bearer" | "header" | "query"; headerName: string; docsUrl: string | null; /** Further key headers (an app id beside the access token); each value is entered in the native dialog. */ extraHeaderNames?: string[]; }
export interface ApiExample { label: string; url: string; note: string; ok: boolean | null; result: string | null; testedAtMs: number | null; }
export interface ApiDebug { steps: { label: string; ok: boolean; detail: string }[]; verdict: string; tryExamples: boolean; /** A problem with the saved keys was found and repaired. */ fixed: boolean; }
export interface ApiProposal { name: string; url: string; auth: string; headerName: string; docsUrl: string; method: string; setupInstructions: string[]; prerequisites: string[]; coverage: string; extraHeaderNames?: string[]; /** From the provider documentation: public, free_personal, free_tier, paid_or_approval or unknown. */ cost?: string; }
export interface AccessReport { state: string; summary: string; connectorId: string | null; api: ApiProposal | null; sources: string[]; searched: boolean; checkedAtMs: number; binding: string; /** A saved API on the same service whose keys already work at the proposed address. */ reuseKeysFrom?: string | null; }
export interface AccessPreview { payloadText: string; payloadHash: string; recipient: string; searchAvailable: boolean; }
export interface ConnectionSource {name:string;connectorId:string;toolName:string;argsJson:string;}
export interface ModelStep {purpose:string;prompt:string;}
export interface Program { connectionSources?:ConnectionSource[]; modelStep?:ModelStep|null; summary: string; output_name: string; code: string; webSources: WebSource[]; }
export interface TaskDraft {
  accessPlan?: AccessReport | null;
  /** Set while editing a saved tool: saving its build adds a new version to that tool. */
  replacesToolId?: string | null;
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

export interface OutboundSummary {
  recipient: string;
  model: string;
  includes: Array<{label: string; detail?: string | null; real: boolean}>;
  laterRuns: string[];
  cost: string;
}

export interface OutboundPreview {
  summary: OutboundSummary;
  draftId: string;
  route: ModelRoute;
  model: string;
  destination: string; // Endpoint host, Meta API, Muse plan, or local execution.
  /** Exact text that will be sent (direct route) or supplied to Muse (muse route). Synthetic shape and explicitly reviewed user example only. */
  payloadText: string;
  containsRealRows: boolean;
  estimatedCostMicros: number | null; // null for plan allowance or an unpriced custom endpoint.
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
  testModelCost?: ModelCostSummary | null;
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
      | "missing_data"
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

/** Wording changes made on the plan screen; steps can be reworded or removed, never added. */
export interface RecordingPlanEdit {
  goal: string;
  step_actions: Record<string, string>;
  removed_steps: string[];
  note?: string | null;
}

/** An unverified suggestion from the public MCP registry. */
export interface ConnectorCandidate {
  name: string;
  description: string;
  url: string;
  publisher: string;
}

/** Micro-dollars of model calls with a known price; unknown-price calls are counted apart. */
export interface ModelCostSummary {
  testMicros: number | null;
  testCalls: number;
  testUnknownCalls: number;
  lastRunMicros: number | null;
  lastRunCalls: number;
  lastRunUnknownCalls: number;
  runCount: number;
  totalMicros: number;
  unknownCalls: number;
}

/** One saved version of a tool; the active one is what runs. */
export interface ToolVersionInfo {
  hash: string;
  number: number;
  createdAtMs: number;
  active: boolean;
  goal: string;
}

export interface ToolDetail extends ToolSummary {
  /** The description the tool was built from, so it can be edited and rebuilt. */
  goal: string;
  modelCost?: ModelCostSummary | null;
  modelStepCap: number;
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
  registration: "dynamic" | "preregistered" | "api";
  verification: "verified-docs" | "verified-live" | "unverified";
  /** For preregistered servers: one or two sentences on creating an OAuth client. */
  clientSetup: string | null;
  /** Loopback redirect the user must register for preregistered clients (fixed port), if any. */
  redirectUri: string | null;
  docsUrl: string | null;
  state: ConnectionState;
  /** For preregistered servers: whether the user has stored a client id. */
  hasClientCredentials: boolean;
  /** Connections to the same service share this name (Gmail by app password and by Google sign-in). */
  service?: string;
  /** Set for email connectors that sign in with an address and an app password. */
  mail?: MailInfo | null;
}

/** Mail server defaults and, once connected, the saved address (never the password). */
export interface MailInfo {
  help: string;
  customServer: boolean;
  imapHost: string;
  imapPort: number;
  smtpHost: string;
  smtpPort: number;
  smtpStarttls: boolean;
  email: string | null;
  /** The provider's own page for creating an app password. */
  appPasswordUrl?: string | null;
}
export interface MailServerInput {
  imapHost: string;
  imapPort: number;
  smtpHost: string;
  smtpPort: number;
  smtpStarttls: boolean;
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
/** Where a scheduled result is emailed: one address, approved with the schedule. */
export interface EmailDelivery { connectorId: string; to: string }
/** Which scheduled runs send the notification and email. Today still records every run. */
export type SendWhen =
  | { kind: "always" }
  | { kind: "changed" }
  | { kind: "contains"; text: string }
  | { kind: "above" | "below"; near: string; side: "after" | "before"; value: number };
export interface ScheduleOutputs { today: boolean; notification: boolean; folder: FileHandle | null; filename: string; email?: EmailDelivery | null; sendWhen?: SendWhen }
export interface ScheduleRequest { frequency: Frequency; outputs: ScheduleOutputs; versionHash: string; approved: boolean }
export interface Schedule { id: string; toolId: string; frequency: Frequency; outputs: ScheduleOutputs; versionHash: string; binding: string; folderIdentity: string | null; status: "enabled" | "paused" | "needs_reapproval" | "emergency_paused"; approvedAtMs: number; lastDueMs: number; nextRunMs: number | null }
/** Entries expire after 30 days; needs_you stays until approved or discarded. */
export interface TodayEntry { id: string; toolId: string; toolName: string; trigger: "schedule" | "manual"; scheduleId: string | null; startedAtMs: number; finishedAtMs: number | null; status: "done" | "needs_you" | "skipped" | "failed" | "staged"; body: string; note: string | null; fileCreated: string | null; runId: string | null; staged: StagedRun | null; notification: boolean; addToToday: boolean; dismissed: boolean }
export interface TodayDay { day: string; entries: TodayEntry[] }

export interface RecordingStatus {
  captureAvailable: boolean;
  helperAvailable: boolean;
  screenRecordingGranted: boolean;
  accessibilityGranted: boolean;
  pickerState: string;
  appName: string | null;
  reason: string | null;
}

export interface Backend {
  accessPreview(draftId: string): Promise<AccessPreview>;
  researchTaskAccess(draftId: string, payloadHash: string): Promise<TaskDraft>;
  /** `keys` are the values typed into the setup form: the main key, then one per extra key name. */
  /** Adds another address on a saved API's own service, reusing its keys; tested before it is kept. */
  addApiAddress(sourceId: string, url: string): Promise<ConnectorInfo>;
  /** Tries a saved API without and with its keys and explains, in plain words, what is wrong. */
  debugApiConnection(id: string): Promise<ApiDebug>;
  /** Example read calls kept beside a custom API, and what each returned (field names only). */
  listApiExamples(id: string): Promise<ApiExample[]>;
  /** Asks the model for example addresses from the API's public documentation. */
  suggestApiExamples(id: string): Promise<ApiExample[]>;
  /** Finds and reads, once each, the paths a task needs on a saved API, keeping only field names. */
  learnApiForTask(id: string, task: string): Promise<ApiExample[]>;
  /** Runs one stored example with the saved keys. */
  runApiExample(id: string, url: string): Promise<ApiExample[]>;
  /** The saved settings of a custom API (never its keys), for editing. */
  getApiConfig(id: string): Promise<ApiConfig>;
  saveApiConnection(config: ApiConfig, keys?: { main: string; extras: string[] }, replaces?: string): Promise<ConnectorInfo>;
  saveApiSearchKey(): Promise<void>;
  removeApiSearchKey(): Promise<void>;
  recordingStatus(): Promise<RecordingStatus>;
  requestRecordingPermission(): Promise<RecordingStatus>;
  confirmNormalBrowserWindow(): Promise<void>;
  removeRecordingFrame(sessionId: string, seq: number): Promise<RecordingFrames>;
  saveRecordingFrame(sessionId: string, seq: number): Promise<string | null>;
  setRecordingMaskLevel(sessionId: string, level: MaskLevel): Promise<void>;
  openPrivacySettings(pane: "screen_recording" | "accessibility"): Promise<void>;
  startRecording(source: "sample" | "window"): Promise<string>;
  stopRecording(sessionId: string): Promise<RecordingFrames>;
  listRecordingFrames(sessionId: string): Promise<RecordingFrames>;
  previewRecording(sessionId: string): Promise<RecordingPreview>;
  consentAndAnalyze(sessionId: string, payloadHash: string): Promise<TaskDraftV2>;
  answerRecordingQuestions(sessionId: string, answers: Record<string, string>): Promise<string>;
  editRecordingDraft(sessionId: string, edit: RecordingPlanEdit): Promise<TaskDraftV2>;
  approveRecordingDraft(sessionId: string): Promise<TaskDraft>;
  discardRecording(sessionId: string): Promise<void>;
  listToday(): Promise<TodayDay[]>;
  dismissTodayEntry(id: string): Promise<void>;
  listSchedules(): Promise<Schedule[]>;
  setSchedule(toolId: string, request: ScheduleRequest): Promise<Schedule>;
  scheduleStatus(toolId: string, status: "enabled" | "paused" | "remove"): Promise<void>;
  loginEnabled(): Promise<boolean>;
  setLoginEnabled(enabled: boolean): Promise<void>;
  appInfo(): Promise<AppInfo>;
  listBotDesigns(): Promise<BotDesign[]>;
  getBotDesign(id: string): Promise<BotDesign>;
  removeBotBackgrounds(id: string): Promise<BotDesign>;
  generateBotImage(request: BotImageRequest): Promise<BotDesign>;

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
  saveCustomEndpoint(baseUrl: string, model: string): Promise<ModelAccessStatus>;
  saveCustomEndpointToken(): Promise<ModelAccessStatus>;
  /** Native dialogs ask for the Modal token ID, then the secret; stored as one `ID.SECRET` bearer. */
  saveCustomEndpointModalToken(): Promise<ModelAccessStatus>;
  setCustomEndpointOptions(displayName: string | null, monthlyCallCap: number): Promise<ModelAccessStatus>;
  testCustomEndpoint(): Promise<ModelAccessStatus>;
  removeCustomEndpoint(): Promise<ModelAccessStatus>;
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
  approveBuildSources(buildId: string, program: Program): Promise<BuildState>;
  setModelStepCap(toolId: string, cap: number): Promise<ToolDetail>;
  stopBuild(buildId: string): Promise<BuildState>;
  saveTool(buildId: string, name: string): Promise<ToolDetail>;

  // Tools and runs
  listTools(): Promise<ToolSummary[]>;
  getTool(toolId: string): Promise<ToolDetail>;
  installSampleTool(): Promise<ToolDetail>;
  exportTool(toolId: string): Promise<string | null>; // null when the folder picker is canceled
  revokeTool(toolId: string): Promise<ToolDetail>;
  restoreTool(toolId: string): Promise<ToolDetail>;
  /** Marks a draft as an edit of a saved tool; saving its build then adds a version to that tool. */
  setDraftReplaces(draftId: string, toolId: string): Promise<TaskDraft>;
  /** Saved versions of a tool, newest first. */
  listToolVersions(toolId: string): Promise<ToolVersionInfo[]>;
  /** Makes an older version the one that runs; nothing is deleted. */
  restoreToolVersion(toolId: string, hash: string): Promise<ToolDetail>;
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
  /** Runs a no-input tool into a discarded scratch folder so its answer can be read first. */
  previewRunNoInput(toolId: string): Promise<StagedRun | { failure: RunFailure }>;
  /** Stages a no-input tool's connection actions only, with no output folder or file. */
  stageActionsNoInput(toolId: string): Promise<StagedRun | { failure: RunFailure }>;
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
  connectMail(connectorId: string, email: string, password: string, server?: MailServerInput | null): Promise<ConnectorInfo>;
  findConnectors(query: string): Promise<ConnectorCandidate[]>;
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

// Stage B recording ingress is raw, memory-only. Never cache or log RecordedFrameInput.
export interface RecordingRect { x: number; y: number; width: number; height: number }
export interface RecordedFrameInput {
  seq: number; monoMs: number; appName: string; windowTitle: string;
  width: number; height: number; rgba: number[]; axLabels: string[];
  ocrBoxes: {text: string; rect: RecordingRect}[];
  masks: {rect: RecordingRect; reason: "secure_field" | "manual"}[];
  uncertain: boolean; secureInputGap: boolean;
}
export interface RecordingFrame {
  seq: number; pngBase64: string; width: number; height: number; unverified?: boolean;
  masks: {rect: RecordingRect; reason: "secure_field" | "manual" | "sensitive_text"}[];
}
export type MaskLevel = "off" | "low" | "standard" | "strict";
export interface RecordingFrames {
  frames: RecordingFrame[]; dropped: {seq: number; reason: string}[]; stopped: boolean; maskLevel: MaskLevel;
}
export interface RecordingPreview {
  sessionId: string; frames: RecordingFrame[]; dropped: RecordingFrames["dropped"];
  clearedTextJson: string; systemInstructions: string; schemaJson: string; payloadText: string;
  totalBytes: number; recipient: string; model: string; payloadHash: string; sample: boolean;
  cost: {estimated_input_tokens: number; max_output_tokens: number; estimated_micros: number; caveat: string};
}
/** Task Draft v2 deliberately preserves the Stage A schema's snake_case fields. */
export interface TaskDraftV2 {
  schema_version: 2; goal: string;
  steps: {id: string; order: number; app: string; action: string; capability: "file_read" | "web_source" | "connection" | "model_step" | "output_file" | "manual"; evidence_frames: number[]}[];
  variables: {name: string; description: string}[]; constants: {name: string; value: string}[];
  output_description: string; example_output?: string | null; trigger: string;
  unsupported_steps: {step_id: string; reason: string}[]; ambiguities: string[];
  questions: {id: string; prompt: string; evidence_frames: number[]; default: string;
    options: {id: string; label: string; any_is_fine: boolean}[]}[];
}

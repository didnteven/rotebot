import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { Backend, BuildState, ShowState } from "./types";

/** Error thrown for backend failures; `message` is safe to show, `code` is stable. */
export class BackendError extends Error {
  constructor(
    message: string,
    readonly code: string,
  ) {
    super(message);
    this.name = "BackendError";
  }
}

/** Commands reject with `{code, message}`; turn that into an Error the UI can show. */
export function toBackendError(e: unknown): Error {
  if (e instanceof Error) return e;
  if (e && typeof e === "object" && "message" in e) {
    const { message, code } = e as { message: unknown; code?: unknown };
    if (typeof message === "string")
      return new BackendError(
        message,
        typeof code === "string" ? code : "error",
      );
  }
  if (typeof e === "string") return new BackendError(e, "error");
  return new BackendError("Something went wrong. Try again.", "unknown");
}

/** Normalize the program DTO only; never rename keys in untrusted schemas or call arguments. */
export function normalizeWire(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(normalizeWire);
  if (value && typeof value === "object" && "program" in value && value.program && typeof value.program === "object") {
    const p=value.program as Record<string,unknown>;
    const {web_sources,connection_sources,model_step,...rest}=p;
    return {...value,program:{...rest,webSources:web_sources??p.webSources??[],connectionSources:connection_sources??p.connectionSources??[],modelStep:model_step??p.modelStep??null}};
  }
  return value;
}
function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const call =
    args === undefined ? tauriInvoke<T>(cmd) : tauriInvoke<T>(cmd, args);
  return Promise.resolve(call).then(v=>normalizeWire(v) as T).catch((e: unknown) => {
    throw toBackendError(e);
  });
}

/** Arguments intentionally use camelCase: Tauri maps them to Rust parameters. */
export class TauriBackend implements Backend {
  recordingStatus: Backend["recordingStatus"] = () => invoke("recording_status");
  confirmNormalBrowserWindow: Backend["confirmNormalBrowserWindow"] = () => invoke("confirm_normal_browser_window");
  openPrivacySettings: Backend["openPrivacySettings"] = pane => invoke("open_privacy_settings", { pane });
  requestRecordingPermission: Backend["requestRecordingPermission"] = () => invoke("request_recording_permission");
  startRecording: Backend["startRecording"] = source => invoke("start_recording", {source});
  stopRecording: Backend["stopRecording"] = sessionId => invoke("stop_recording", {sessionId});
  listRecordingFrames: Backend["listRecordingFrames"] = sessionId => invoke("list_recording_frames", {sessionId});
  previewRecording: Backend["previewRecording"] = sessionId => invoke("preview_recording", {sessionId});
  consentAndAnalyze: Backend["consentAndAnalyze"] = (sessionId, payloadHash) => invoke("consent_and_analyze", {sessionId, payloadHash});
  answerRecordingQuestions: Backend["answerRecordingQuestions"] = (sessionId, answers) => invoke("answer_recording_questions", {sessionId, answers});
  approveRecordingDraft: Backend["approveRecordingDraft"] = sessionId => invoke("approve_recording_draft", {sessionId});
  discardRecording: Backend["discardRecording"] = sessionId => invoke("discard_recording", {sessionId});
  listToday: Backend["listToday"] = () => invoke("list_today");
  dismissTodayEntry: Backend["dismissTodayEntry"] = (id) => invoke("dismiss_today_entry", {id});
  listSchedules: Backend["listSchedules"] = () => invoke("list_schedules");
  setSchedule: Backend["setSchedule"] = (toolId, request) => invoke("set_schedule", {toolId, request});
  scheduleStatus: Backend["scheduleStatus"] = (toolId, status) => invoke("schedule_status", {toolId, status});
  loginEnabled: Backend["loginEnabled"] = () => invoke("login_enabled");
  setLoginEnabled: Backend["setLoginEnabled"] = (enabled) => invoke("set_login_enabled", {enabled});
  appInfo: Backend["appInfo"] = () => invoke("app_info");
  modelAccess: Backend["modelAccess"] = () => invoke("model_access");
  enterApiKeyNative: Backend["enterApiKeyNative"] = () =>
    invoke("enter_api_key_native");
  saveApiKey: Backend["saveApiKey"] = (key) => invoke("save_api_key", { key });
  removeApiKey: Backend["removeApiKey"] = () => invoke("remove_api_key");
  testApiKey: Backend["testApiKey"] = () => invoke("test_api_key");
  refreshMuseStatus: Backend["refreshMuseStatus"] = () =>
    invoke("refresh_muse_status");
  openMuseSignIn: Backend["openMuseSignIn"] = () => invoke("open_muse_sign_in");
  saveCustomEndpoint: Backend["saveCustomEndpoint"] = (baseUrl, model) => invoke("save_custom_endpoint", {baseUrl, model});
  saveCustomEndpointToken: Backend["saveCustomEndpointToken"] = () => invoke("save_custom_endpoint_token");
  setCustomEndpointOptions: Backend["setCustomEndpointOptions"] = (displayName, monthlyCallCap) => invoke("set_custom_endpoint_options", {displayName, monthlyCallCap});
  testCustomEndpoint: Backend["testCustomEndpoint"] = () => invoke("test_custom_endpoint");
  removeCustomEndpoint: Backend["removeCustomEndpoint"] = () => invoke("remove_custom_endpoint");
  selectRoute: Backend["selectRoute"] = (route) =>
    invoke("select_route", { route });
  setMuseUnverifiedOk: Backend["setMuseUnverifiedOk"] = (ok) =>
    invoke("set_muse_unverified_ok", { ok });
  setLocalOnly: Backend["setLocalOnly"] = (localOnly) =>
    invoke("set_local_only", { localOnly });
  pickCsv: Backend["pickCsv"] = () => invoke("pick_csv");
  loadSampleCsv: Backend["loadSampleCsv"] = () => invoke("load_sample_csv");
  draftTask: Backend["draftTask"] = (input, description) =>
    invoke("draft_task", { input, description });
  draftTaskNoInput: Backend["draftTaskNoInput"] = (description) =>
    invoke("draft_task_no_input", { description });
  answerQuestion: Backend["answerQuestion"] = (draftId, questionId, answer) =>
    invoke("answer_question", { draftId, questionId, answer });
  updateMapping: Backend["updateMapping"] = (draftId, mapping, outputName) =>
    invoke("update_mapping", { draftId, mapping, outputName });
  selectTaskConnections: Backend["selectTaskConnections"] = (draftId, connectorIds) =>
    invoke("select_task_connections", { draftId, connectorIds });
  updateTask: Backend["updateTask"] = (draftId, kind, outputName, exampleOutput) =>
    invoke("update_task", { draftId, kind, outputName, exampleOutput });
  outboundPreview: Backend["outboundPreview"] = (draftId) =>
    invoke("outbound_preview", { draftId });
  startBuild: Backend["startBuild"] = (draftId, consent) =>
    invoke("start_build", { draftId, consent });
  buildState: Backend["buildState"] = (buildId) =>
    invoke("build_state", { buildId });
  onBuildEvent: Backend["onBuildEvent"] = (buildId, cb) =>
    listen<BuildState>("build://" + buildId, (event) => cb(normalizeWire(event.payload) as BuildState));
  buildTryInput: Backend["buildTryInput"] = (buildId, input) =>
    invoke("build_try_input", { buildId, input });
  stopBuild: Backend["stopBuild"] = (buildId) =>
    invoke("stop_build", { buildId });
  approveBuildSources: Backend["approveBuildSources"] = (buildId, program) => {
    const {webSources,connectionSources,modelStep,...rest}=program;
    return invoke("approve_build_sources", {buildId,program:{...rest,web_sources:webSources,connection_sources:connectionSources??[],model_step:modelStep??null}});
  };
  setModelStepCap: Backend["setModelStepCap"] = (toolId,cap) => invoke("set_model_step_cap",{toolId,cap});
  saveTool: Backend["saveTool"] = (buildId, name) =>
    invoke("save_tool", { buildId, name });
  listTools: Backend["listTools"] = () => invoke("list_tools");
  getTool: Backend["getTool"] = (toolId) => invoke("get_tool", { toolId });
  installSampleTool: Backend["installSampleTool"] = () =>
    invoke("install_sample_tool");
  exportTool: Backend["exportTool"] = (toolId) =>
    invoke("export_tool", { toolId });
  revokeTool: Backend["revokeTool"] = (toolId) =>
    invoke("revoke_tool", { toolId });
  restoreTool: Backend["restoreTool"] = (toolId) =>
    invoke("restore_tool", { toolId });
  deleteTool: Backend["deleteTool"] = (toolId) =>
    invoke("delete_tool", { toolId });
  pickOutputDir: Backend["pickOutputDir"] = () => invoke("pick_output_dir");
  stageRun: Backend["stageRun"] = (toolId, input, outputDir, outputName) =>
    invoke("stage_run", { toolId, input, outputDir, outputName });
  stageRunNoInput: Backend["stageRunNoInput"] = (toolId, outputDir, outputName) =>
    invoke("stage_run_no_input", { toolId, outputDir, outputName });
  commitRun: Backend["commitRun"] = (runId, planHash, actionsReviewed = false) =>
    invoke("commit_run", { runId, planHash, actionsReviewed });
  previewRunNoInput: Backend["previewRunNoInput"] = (toolId) =>
    invoke("preview_run_no_input", { toolId });
  discardRun: Backend["discardRun"] = (runId) =>
    invoke("discard_run", { runId });
  revealOutput: Backend["revealOutput"] = (receiptRunId) =>
    invoke("reveal_output", { receiptRunId });
  listActivity: Backend["listActivity"] = () => invoke("list_activity");
  listReceipts: Backend["listReceipts"] = () => invoke("list_receipts");
  privacyOverview: Backend["privacyOverview"] = () =>
    invoke("privacy_overview");
  deletePreview: Backend["deletePreview"] = () => invoke("delete_preview");
  deleteData: Backend["deleteData"] = (options) =>
    invoke("delete_data", { options });
  deleteOutboundHistory: Backend["deleteOutboundHistory"] = () =>
    invoke("delete_outbound_history");
  emergencyStop: Backend["emergencyStop"] = () => invoke("emergency_stop");
  openExternal: Backend["openExternal"] = (url) =>
    invoke("open_external", { url });
  previewConnectorTools: Backend["previewConnectorTools"] = (connectorId) =>
    invoke("preview_connector_tools", { connectorId });
  listConnectors: Backend["listConnectors"] = () => invoke("list_connectors");
  setConnectorUrl: Backend["setConnectorUrl"] = (connectorId, url) =>
    invoke("set_connector_url", { connectorId, url });
  setClientCredentials: Backend["setClientCredentials"] = (
    connectorId,
    clientId,
    clientSecret,
  ) =>
    invoke("set_client_credentials", { connectorId, clientId, clientSecret });
  beginConnect: Backend["beginConnect"] = (connectorId) =>
    invoke("begin_connect", { connectorId });
  completeConnect: Backend["completeConnect"] = (pendingId) =>
    invoke("complete_connect", { pendingId });
  testConnection: Backend["testConnection"] = (connectorId) =>
    invoke("test_connection", { connectorId });
  disconnect: Backend["disconnect"] = (connectorId) =>
    invoke("disconnect", { connectorId });
  connectionSuggestions: Backend["connectionSuggestions"] = () =>
    invoke("connection_suggestions");
  dismissSuggestion: Backend["dismissSuggestion"] = (connectorId) =>
    invoke("dismiss_suggestion", { connectorId });
  showState: Backend["showState"] = () => invoke("show_state");
  startShow: Backend["startShow"] = () => invoke("start_show");
  pauseShow: Backend["pauseShow"] = (paused) =>
    invoke("pause_show", { paused });
  stopShow: Backend["stopShow"] = () => invoke("stop_show");
  onShowEvent: Backend["onShowEvent"] = (cb) =>
    listen<ShowState>("show://state", (event) => cb(event.payload));
}

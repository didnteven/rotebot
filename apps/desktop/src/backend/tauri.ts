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

function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const call =
    args === undefined ? tauriInvoke<T>(cmd) : tauriInvoke<T>(cmd, args);
  return Promise.resolve(call).catch((e: unknown) => {
    throw toBackendError(e);
  });
}

/** Arguments intentionally use camelCase: Tauri maps them to Rust parameters. */
export class TauriBackend implements Backend {
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
    listen<BuildState>("build://" + buildId, (event) => cb(event.payload));
  buildTryInput: Backend["buildTryInput"] = (buildId, input) =>
    invoke("build_try_input", { buildId, input });
  stopBuild: Backend["stopBuild"] = (buildId) =>
    invoke("stop_build", { buildId });
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

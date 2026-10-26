import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { Backend, BuildState, ShowState } from "./types";

/** Arguments intentionally use camelCase: Tauri maps them to Rust parameters. */
export class TauriBackend implements Backend {
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
  setLocalOnly: Backend["setLocalOnly"] = (localOnly) =>
    invoke("set_local_only", { localOnly });
  pickCsv: Backend["pickCsv"] = () => invoke("pick_csv");
  loadSampleCsv: Backend["loadSampleCsv"] = () => invoke("load_sample_csv");
  draftTask: Backend["draftTask"] = (input, description) =>
    invoke("draft_task", { input, description });
  answerQuestion: Backend["answerQuestion"] = (draftId, questionId, answer) =>
    invoke("answer_question", { draftId, questionId, answer });
  updateMapping: Backend["updateMapping"] = (draftId, mapping, outputName) =>
    invoke("update_mapping", { draftId, mapping, outputName });
  outboundPreview: Backend["outboundPreview"] = (draftId) =>
    invoke("outbound_preview", { draftId });
  startBuild: Backend["startBuild"] = (draftId, consent) =>
    invoke("start_build", { draftId, consent });
  buildState: Backend["buildState"] = (buildId) =>
    invoke("build_state", { buildId });
  onBuildEvent: Backend["onBuildEvent"] = (buildId, cb) =>
    listen<BuildState>("build://" + buildId, (event) => cb(event.payload));
  stopBuild: Backend["stopBuild"] = (buildId) =>
    invoke("stop_build", { buildId });
  saveTool: Backend["saveTool"] = (buildId, name) =>
    invoke("save_tool", { buildId, name });
  listTools: Backend["listTools"] = () => invoke("list_tools");
  getTool: Backend["getTool"] = (toolId) => invoke("get_tool", { toolId });
  installSampleTool: Backend["installSampleTool"] = () =>
    invoke("install_sample_tool");
  deleteTool: Backend["deleteTool"] = (toolId) =>
    invoke("delete_tool", { toolId });
  pickOutputDir: Backend["pickOutputDir"] = () => invoke("pick_output_dir");
  stageRun: Backend["stageRun"] = (toolId, input, outputDir, outputName) =>
    invoke("stage_run", { toolId, input, outputDir, outputName });
  commitRun: Backend["commitRun"] = (runId, planHash) =>
    invoke("commit_run", { runId, planHash });
  discardRun: Backend["discardRun"] = (runId) =>
    invoke("discard_run", { runId });
  revealOutput: Backend["revealOutput"] = (receiptRunId) =>
    invoke("reveal_output", { receiptRunId });
  listActivity: Backend["listActivity"] = () => invoke("list_activity");
  listReceipts: Backend["listReceipts"] = () => invoke("list_receipts");
  privacyOverview: Backend["privacyOverview"] = () =>
    invoke("privacy_overview");
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

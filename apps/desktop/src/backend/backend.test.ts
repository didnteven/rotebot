import { afterEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { createBackend } from "./index";
import { MockBackend } from "./mock";
import { TauriBackend } from "./tauri";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

afterEach(() => {
  Reflect.deleteProperty(window, "__TAURI_INTERNALS__");
});

describe("typed backend seam", () => {
  it("selects the memory backend in a browser", async () => {
    const backend = createBackend();
    expect(backend).toBeInstanceOf(MockBackend);
    expect(await backend.appInfo()).toEqual({
      version: "0.1.0",
      platform: "browser",
      mock: true,
      secretStore: "memory",
      debugBuild: false,
    });
    expect(await backend.listTools()).toEqual([]);
    expect(invoke).not.toHaveBeenCalled();
  });

  it("selects native IPC in Tauri and maps both command names", async () => {
    Object.defineProperty(window, "__TAURI_INTERNALS__", {
      value: {},
      configurable: true,
    });
    const backend = createBackend();
    expect(backend).toBeInstanceOf(TauriBackend);
    vi.mocked(invoke)
      .mockResolvedValueOnce({ version: "0.1.0", platform: "macos" })
      .mockResolvedValueOnce([]);
    expect(await backend.appInfo()).toEqual({
      version: "0.1.0",
      platform: "macos",
    });
    expect(await backend.listTools()).toEqual([]);
    expect(invoke).toHaveBeenNthCalledWith(1, "app_info");
    expect(invoke).toHaveBeenNthCalledWith(2, "list_tools");
  });

  it("isolates memory state from constructor inputs and returned objects", async () => {
    const tools = [{ id: "tool-1", name: "Orders", version: 1 }];
    const backend = new MockBackend(tools);
    tools[0].name = "changed";
    const result = await backend.listTools();
    result[0].name = "also changed";
    expect((await backend.listTools())[0].name).toBe("Orders");
    expect(await new MockBackend().listTools()).toEqual([]);
  });
});

it("forwards every command with the shared camelCase arguments and snake_case names", async () => {
  vi.mocked(invoke).mockReset();
  const backend = new TauriBackend();
  const cases: [string, unknown[], Record<string, unknown> | undefined][] = [
    ["appInfo", [], undefined],
    ["modelAccess", [], undefined],
    ["enterApiKeyNative", [], undefined],
    ["saveApiKey", ["key"], { key: "key" }],
    ["removeApiKey", [], undefined],
    ["testApiKey", [], undefined],
    ["refreshMuseStatus", [], undefined],
    ["openMuseSignIn", [], undefined],
    ["selectRoute", ["offline_demo"], { route: "offline_demo" }],
    ["setLocalOnly", [true], { localOnly: true }],
    ["setMuseUnverifiedOk", [true], { ok: true }],
    ["pickCsv", [], undefined],
    ["loadSampleCsv", [], undefined],
    [
      "draftTask",
      [{ handleId: "file" }, "Clean"],
      { input: { handleId: "file" }, description: "Clean" },
    ],
    [
      "draftTaskNoInput",
      ["Get the forecast"],
      { description: "Get the forecast" },
    ],
    [
      "answerQuestion",
      ["d", "q", "a"],
      { draftId: "d", questionId: "q", answer: "a" },
    ],
    [
      "updateMapping",
      ["d", {}, "out.csv"],
      { draftId: "d", mapping: {}, outputName: "out.csv" },
    ],
    ["outboundPreview", ["d"], { draftId: "d" }],
    [
      "startBuild",
      ["d", { outboundReviewed: true }],
      { draftId: "d", consent: { outboundReviewed: true } },
    ],
    [
      "buildTryInput",
      ["b", { handleId: "f" }],
      { buildId: "b", input: { handleId: "f" } },
    ],
    ["buildState", ["b"], { buildId: "b" }],
    ["setModelStepCap", ["t",24], {toolId:"t",cap:24}],
    ["approveBuildSources", ["b",{summary:"s",output_name:"summary.txt",code:"function run(){}",webSources:[]}], {buildId:"b",program:{summary:"s",output_name:"summary.txt",code:"function run(){}",web_sources:[],connection_sources:[],model_step:null}}],
    ["stopBuild", ["b"], { buildId: "b" }],
    ["saveTool", ["b", "Name"], { buildId: "b", name: "Name" }],
    ["listTools", [], undefined],
    ["getTool", ["t"], { toolId: "t" }],
    ["installSampleTool", [], undefined],
    ["exportTool", ["t"], { toolId: "t" }],
    ["revokeTool", ["t"], { toolId: "t" }],
    ["restoreTool", ["t"], { toolId: "t" }],
    ["deletePreview", [], undefined],
    [
      "deleteData",
      [{ includeTools: true }],
      { options: { includeTools: true } },
    ],
    ["deleteTool", ["t"], { toolId: "t" }],
    ["pickOutputDir", [], undefined],
    [
      "stageRun",
      ["t", {}, {}, "out.csv"],
      { toolId: "t", input: {}, outputDir: {}, outputName: "out.csv" },
    ],
    [
      "stageRunNoInput",
      ["t", {}, "weather.txt"],
      { toolId: "t", outputDir: {}, outputName: "weather.txt" },
    ],
    ["commitRun", ["r", "hash"], { runId: "r", planHash: "hash", actionsReviewed: false }],
    ["commitRun", ["r", "hash", true], { runId: "r", planHash: "hash", actionsReviewed: true }],
    ["selectTaskConnections", ["d", ["gmail"]], { draftId:"d", connectorIds:["gmail"] }],
    ["discardRun", ["r"], { runId: "r" }],
    ["revealOutput", ["r"], { receiptRunId: "r" }],
    ["listActivity", [], undefined],
    ["listReceipts", [], undefined],
    ["privacyOverview", [], undefined],
    ["deleteOutboundHistory", [], undefined],
    ["emergencyStop", [], undefined],
    ["openExternal", ["https://dev.meta.ai/"], { url: "https://dev.meta.ai/" }],
    ["previewConnectorTools", ["gmail"], { connectorId: "gmail" }],
    ["listConnectors", [], undefined],
    [
      "setConnectorUrl",
      ["gmail", "https://example.com/mcp"],
      { connectorId: "gmail", url: "https://example.com/mcp" },
    ],
    [
      "setClientCredentials",
      ["gmail", "id", "secret"],
      { connectorId: "gmail", clientId: "id", clientSecret: "secret" },
    ],
    ["beginConnect", ["gmail"], { connectorId: "gmail" }],
    ["completeConnect", ["p"], { pendingId: "p" }],
    ["testConnection", ["gmail"], { connectorId: "gmail" }],
    ["disconnect", ["gmail"], { connectorId: "gmail" }],
    ["connectionSuggestions", [], undefined],
    ["dismissSuggestion", ["gmail"], { connectorId: "gmail" }],
    ["recordingStatus", [], undefined],
    ["startRecording", ["sample"], {source:"sample"}],
    ["submitFrame", ["s", {seq:1, rgba:[0,0,0,255]}], {sessionId:"s", frame:{seq:1, rgba:[0,0,0,255]}}],
    ["stopRecording", ["s"], {sessionId:"s"}],
    ["listRecordingFrames", ["s"], {sessionId:"s"}],
    ["previewRecording", ["s"], {sessionId:"s"}],
    ["consentAndAnalyze", ["s","hash"], {sessionId:"s",payloadHash:"hash"}],
    ["answerRecordingQuestions", ["s",{format:"text"}], {sessionId:"s",answers:{format:"text"}}],
    ["approveRecordingDraft", ["s"], {sessionId:"s"}],
    ["discardRecording", ["s"], {sessionId:"s"}],
    ["showState", [], undefined],
    ["startShow", [], undefined],
    ["pauseShow", [true], { paused: true }],
    ["stopShow", [], undefined],
  ];
  for (const [name, args, expected] of cases) {
    await (
      backend as unknown as Record<
        string,
        (...args: unknown[]) => Promise<unknown>
      >
    )[name](...args);
    const command = name.replace(
      /[A-Z]/g,
      (letter) => "_" + letter.toLowerCase(),
    );
    if (expected) expect(invoke).toHaveBeenLastCalledWith(command, expected);
    else expect(invoke).toHaveBeenLastCalledWith(command);
  }
  expect(invoke).toHaveBeenCalledTimes(cases.length);
});

it("provides verified catalog metadata and public Gmail tool previews", async () => {
  const backend = new MockBackend();
  const connectors = await backend.listConnectors();
  for (const connector of connectors) {
    expect(connector.verification).toBe(
      connector.id === "custom" ? "unverified" : "verified-live",
    );
    const preregistered = [
      "gmail",
      "google-calendar",
      "google-drive",
      "slack",
      "github",
    ].includes(connector.id);
    expect(connector.registration).toBe(
      preregistered ? "preregistered" : "dynamic",
    );
    expect(connector.clientSetup).toEqual(
      preregistered ? expect.any(String) : null,
    );
    expect(connector.redirectUri).toBe(
      ["slack", "github"].includes(connector.id)
        ? "http://127.0.0.1:53682/callback"
        : null,
    );
  }
  expect(
    (await backend.previewConnectorTools("gmail")).map((tool) => tool.name),
  ).toEqual(["Create draft email", "Search threads", "Read thread"]);
  expect(await backend.previewConnectorTools("notion")).toEqual([]);
});

it("passes the host's outbound summary through IPC without parsing the payload",async()=>{
  const backend=new MockBackend();await backend.saveApiKey("test-key");
  const input=backend.importText("input.txt","Private data");
  const draft=await backend.draftTask(input.handle,"Extract errors");
  await backend.updateTask(draft.draftId,"program","errors.txt","Exact real example");
  const preview={...await backend.outboundPreview(draft.draftId),payloadText:"an opaque exact payload"};
  vi.mocked(invoke).mockResolvedValueOnce(preview);
  const result=await new TauriBackend().outboundPreview(draft.draftId);
  expect(result).toEqual(preview);
  expect(result.summary.includes).toContainEqual({label:"Your example result",detail:"Exact real example",real:true});
  expect(invoke).toHaveBeenLastCalledWith("outbound_preview",{draftId:draft.draftId});
});

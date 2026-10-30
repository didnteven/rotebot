import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { App } from "./App";
import { MockBackend } from "./backend/mock";
import type { Backend, BuildState, CsvPreview, Receipt, TaskDraft } from "./backend/types";

const payloads = [
  ["image handler", "<img src=x onerror=alert(1)>"],
  ["script", "<script>alert(1)</script>"],
  ["JavaScript URL", "javascript:alert(1)"],
  ["SVG handler", "<svg onload=alert(1)>"],
  ["CSS resource", "url(https://evil.example/x)"],
  ["10,000 characters", "X".repeat(10_000)],
  ["RTL override", "report\u202Eexe.csv"],
  ["zero width", "re\u200Bpo\u200Crt\u200D\u2060\uFEFF.csv"],
  ["terminal escape", "\u001b[31mred"],
  ["Markdown link", "[x](javascript:alert(1))"],
] as const;

// Independent expected display rule: paths expose hidden controls without changing
// backend values, file handles, selected names or committed bytes.
const pathDisplay = (value: string) => value.replace(
  /[\u001b\u200b-\u200f\u202a-\u202e\u2060-\u206f\ufeff]/g,
  (c) => `\\u${c.charCodeAt(0).toString(16).padStart(4, "0")}`,
);
const textOptions = { exact: true, normalizer: (text: string) => text.trim() };
const button = (name: string) => screen.getByRole("button", { name });
const commands = ["startBuild", "buildTryInput", "beginConnect", "completeConnect", "openExternal", "exportTool", "stageRun", "commitRun", "revealOutput", "startShow", "openMuseSignIn", "saveApiKey"] as const;

function assertBounded(element: HTMLElement) {
  const style = getComputedStyle(element);
  expect(
    style.overflowWrap === "anywhere" ||
    ["break-all", "break-word"].includes(style.wordBreak) ||
    (style.maxWidth !== "" && style.maxWidth !== "none"),
    `Unbounded ${element.tagName} at ${element.parentElement?.tagName}`,
  ).toBe(true);
}
function literal(scope: HTMLElement, value: string, path = false) {
  const elements = within(scope).getAllByText(path ? pathDisplay(value) : value, textOptions);
  for (const element of elements) {
    expect(element).toBeVisible();
    assertBounded(element);
    if (path) {
      expect(element.tagName).toBe("BDI");
      expect(element).toHaveAttribute("dir", "ltr");
      expect(element.textContent).not.toMatch(/[\u200b-\u200f\u202a-\u202e\u2060-\u206f\ufeff]/);
    }
  }
}
function inputLiteral(label: string, value: string) {
  const element = screen.getByLabelText(label);
  expect(element).toHaveValue(value);
  expect(screen.getAllByDisplayValue(value, textOptions)).toContain(element);
  assertBounded(element);
}

function fixture(payload: string) {
  const v = (surface: string) => `${surface}: ${payload}`;
  const csv: CsvPreview = {
    handle: { handleId: "csv", displayName: v("file name"), displayPath: v("input path") },
    columns: [v("CSV header"), v("mapping source")],
    // Inject at the IPC boundary so the renderer also faces oversized data even
    // where the normal CSV reader already truncates preview cells to 200 chars.
    rows: [[payload, "source"]], rowCount: 1, byteSize: 100,
  };
  const folder = { handleId: "folder", displayName: "folder", displayPath: v("folder path") };
  const draft: TaskDraft = {
    draftId: "draft", revision: 1, goal: "fixture", input: csv, noInput: false,
    summary: [v("summary")], questions: [], mentionedConnectors: [],
    mapping: {
      output_columns: [{ name: v("mapping column"), source: v("mapping source"), transform: { type: "constant", value: v("constant") } }],
      on_error: "stop_and_explain", skip_header_rows: 0, formula_escape: true,
    },
    outputName: v("output name"), readyToBuild: true, blockers: [],
  };
  const build: BuildState = {
    buildId: "build", draftId: "draft", route: "api_key", routeLabel: "Fixture route", phase: "ready",
    candidate: draft.mapping, checks: [{ id: "check", label: v("check label"), detail: v("check detail"), passed: true }],
    comparison: { inputColumns: csv.columns, inputRows: csv.rows, outputColumns: [v("mapping column")], outputRows: [[v("constant")]], rowsIn: 1, rowsOut: 1 },
    events: [{ buildId: "build", seq: 1, phase: "ready", atMs: 1, message: v("build event") }],
    spentMicros: 0, failure: null,
  };
  const receipt: Receipt = {
    runId: "run", toolId: "tool", toolName: v("receipt tool"), versionHash: "abc", planHash: "def", inputDigest: "ghi", committedDigest: "jkl",
    outputPath: v("receipt path"), status: "committed", atMs: 1,
  };
  const suggestion = { connectorId: "hostile", connectorName: v("suggested connector"), message: v("suggestion"), signalKind: "foreground_app" as const, count: 1 };
  const outbound = { id: "outbound", atMs: 1, purpose: v("outbound purpose"), destination: v("destination"), route: "api_key" as const, syntheticOnly: true, costMicros: 0, payloadText: v("outbound payload") };
  const backend = new MockBackend([{ id: "tool", name: v("saved tool"), version: 1 }], {
    testOnlyOverrides: {
      modelAccess: async () => ({ customEndpoint: {baseUrl:null,host:null,model:null,displayName:null,tokenPresent:false,lastTest:null,monthlyCallCap:500,callsThisMonth:0}, apiKey: { present: false, fingerprint: null, lastTest: null }, muse: { kind: "unknown", version: null, message: v("model error") }, selectedRoute: null, effectiveRoute: "offline_demo", blockedReason: v("route blocker"), egress: "cloud_allowed", model: "fixture", museUnverifiedOk: false }),
      refreshMuseStatus: async () => { throw new Error(v("settings error")); },
      pickCsv: async () => csv,
      pickOutputDir: async () => folder,
      draftTask: async (_input, description) => ({...draft, goal: description}),
      updateMapping: async (_id, mapping, outputName) => ({ ...draft, mapping, outputName }),
      outboundPreview: async () => ({ ...outbound, summary:{recipient:v("destination"),model:v("model"),includes:[],laterRuns:[],cost:"Free"}, draftId: "draft", model: v("model"), containsRealRows: false, estimatedCostMicros: 0, budgetMicros: 1, webHosts: [] }),
      startBuild: async () => build,
      buildState: async () => build,
      buildTryInput: async () => ({ ...build, phase: "failed", failure: { kind: "internal", message: v("build error") } }),
      listConnectors: async () => [{
        id: "hostile", name: v("connector"), category: "custom", mcpUrl: payload, docsUrl: payload,
        registration: "dynamic", verification: "unverified", clientSetup: null, redirectUri: null,
        state: { kind: "error", message: v("connector error") }, hasClientCredentials: false,
      }],
      previewConnectorTools: async () => [{ name: v("MCP tool"), description: v("MCP description"), readOnly: null, destructive: null }],
      beginConnect: async () => ({ connectorId: "hostile", pendingId: "pending", authorizationUrl: payload }),
      // Hold the authorization field open for inspection, without timers or IO.
      completeConnect: () => new Promise(() => {}),
      connectionSuggestions: async () => [suggestion],
      listActivity: async () => [{ id: "activity", kind: "run", atMs: 1, title: v("activity title"), detail: v("activity detail") }],
      listReceipts: async () => [receipt],
      showState: async () => ({ kind: "finished", sessionId: "show", durationSeconds: 1, appsSeen: [v("app")], suggestions: [suggestion] }),
      startShow: async () => ({ kind: "unavailable", reason: v("show error") }),
      privacyOverview: async () => ({ egress: "cloud_allowed", stored: [{ label: v("stored label"), count: 1, bytes: 1, expires: v("expiry") }], outbound: [outbound] }),
      exportTool: async () => v("export path"),
      stageRun: async () => ({ runId: "run", toolId: "tool", input: csv.handle, outputDir: folder, outputName: draft.outputName, planHash: "def", rowsIn: 1, rowsOut: 1, outputColumns: [v("run header")], previewRows: [[v("run cell")]], destinationExists: false }),
      commitRun: async () => receipt,
      revealOutput: async () => { throw new Error(v("action error")); },
    },
  });
  return { backend, v, csv };
}

// Vitest skips ordinary CSS imports. Install the shipped stylesheet so computed
// style assertions exercise real rules (jsdom itself does not perform layout).
const stylesheet = document.createElement("style");
beforeAll(() => {
  stylesheet.textContent = readFileSync(resolve(process.cwd(), "src/styles.css"), "utf8");
  document.head.append(stylesheet);
});
afterAll(() => stylesheet.remove());
afterEach(() => vi.unstubAllGlobals());
beforeEach(() => {
  vi.stubGlobal("fetch", vi.fn(() => { throw new Error("Unexpected renderer network request"); }));
  vi.spyOn(window, "open").mockImplementation(() => null);
  vi.spyOn(window, "alert").mockImplementation(() => {});
});

// Each payload reaches EACH surface through the same Backend seam used by the
// desktop app, with assertions scoped to that surface before navigating away.
describe.each(payloads)("P11 renderer: %s", (_name, payload) => {
  it("keeps CSV, mapping, outbound review, build comparison and event text inert", async () => {
    const { backend, v } = fixture(payload);
    const update = vi.spyOn(backend, "updateMapping");
    const { user, root, inert } = await open(backend);
    await user.click(button("Describe a task"));
    fireEvent.change(screen.getByLabelText("What should the tool do?"), { target: { value: "Map this CSV" } });
    await user.click(button("Choose file"));
    literal(root, v("file name"), true);
    literal(root, v("input path"), true);
    literal(screen.getByRole("region", { name: "Selected CSV · first 8 rows" }), v("CSV header"));
    literal(screen.getByRole("region", { name: "Selected CSV · first 8 rows" }), payload);
    inert();
    await user.click(button("Check the plan"));
    await user.click(screen.getByText("Details"));
    literal(root, v("summary"));
    inputLiteral("Output column 1", v("mapping column"));
    inputLiteral("Constant value 1", v("constant"));
    inputLiteral("Output file name", v("output name"));
    if (pathDisplay(v("output name")) !== v("output name")) literal(root, v("output name"), true);
    literal(screen.getByRole("region", { name: "Column mapping" }), v("mapping source"));
    inert();
    await waitFor(() => expect(button("Send and build")).toBeEnabled());
    await user.click(screen.getByText(/What leaves this Mac ·/));
    expect(update).toHaveBeenCalledWith("draft", expect.any(Object), v("output name"));
    await screen.findByLabelText("Exact data sent");
    await user.click(screen.getByText("Exact data sent"));
    literal(root, v("outbound payload"));
    literal(root, v("model"));
    literal(root, v("destination"));
    inert();
    await user.click(button("Send and build"));
    const result = screen.getByRole("region", { name: "Result · first 8 rows" });
    literal(result, v("mapping column"));
    literal(result, v("constant"));
    await user.click(screen.getByText("Details"));
    // These are plain strings embedded in host-owned rule/check/event text.
    for (const label of ["mapping column", "mapping source", "check label", "check detail"]) {
      const lines = screen.getAllByText((text, node) => node?.tagName === "LI" && text.includes(v(label)), textOptions);
      for (const line of lines) {
        expect(line).toBeVisible();
        assertBounded(line);
      }
    }
    expect(screen.getByText((text, node) => node?.tagName === "LI" && text.includes(v("build event")), textOptions)).toBeVisible();
    inert({ startBuild: 1 });
    await user.click(button("Try another sample"));
    literal(root, v("build error"));
    inert({ startBuild: 1, buildTryInput: 1 });
  });

  it("keeps connector names, URL fields, documentation URLs, tools, errors and suggestions inert", async () => {
    const { backend, v } = fixture(payload);
    const external = vi.spyOn(backend, "openExternal");
    const connect = vi.spyOn(backend, "beginConnect");
    const { user, root, inert } = await open(backend);
    literal(root, v("suggestion"));
    literal(root, `Connect ${v("suggested connector")}`);
    await user.click(button("Connections"));
    literal(root, v("connector"));
    await user.click(screen.getByText("What it can do"));
    await screen.findByText(v("MCP tool"), textOptions);
    literal(root, v("MCP tool"));
    literal(root, v("MCP description"));
    // Select the catalog action rather than the suggestion's action.
    const catalog = screen.getByText(v("connector"), textOptions).closest("article")!;
    await user.click(within(catalog).getByRole("button"));
    // Choosing the service starts its sign-in once; the hostile text stays inert.
    await screen.findByLabelText("Authorization URL");
    expect(connect).toHaveBeenCalledTimes(1);
    inputLiteral("MCP URL", payload);
    literal(root, payload); // URL-like strings must remain inspectable as text.
    literal(screen.getByText("Documentation:", textOptions), payload);
    literal(root, v("connector error"));
    expect(external).not.toHaveBeenCalled();
    inputLiteral("Authorization URL", payload);
    inert({ beginConnect: 1, completeConnect: 1 });
  });

  it("keeps activity, receipts, Show app names, privacy records and export paths inert", async () => {
    const { backend, v } = fixture(payload);
    const { user, root, inert } = await open(backend);
    literal(root, v("saved tool"));
    await user.click(button("Export"));
    literal(root, v("export path"), true);
    await user.click(button("Activity"));
    await screen.findByText(v("activity title"), textOptions);
    literal(root, v("activity title"));
    literal(root, v("activity detail"));
    literal(root, v("receipt path"), true);
    inert({ exportTool: 1 });
    await user.click(button("Show in Finder"));
    await screen.findByRole("alert");
    literal(root, v("action error"));
    inert({ exportTool: 1, revealOutput: 1 });
    await user.click(button("Record a task"));
    await user.click(screen.getByText("Advanced: app activity log"));
    expect(screen.getByText(`Apps seen: ${v("app")}`, textOptions)).toBeVisible();
    assertBounded(screen.getByText(`Apps seen: ${v("app")}`, textOptions));
    inert({ exportTool: 1, revealOutput: 1 });
    await user.click(button("Record another task"));
    literal(root, v("show error"));
    inert({ exportTool: 1, revealOutput: 1, startShow: 1 });
    await user.click(button("Privacy"));
    await screen.findByText(v("outbound purpose"), textOptions);
    literal(root, v("stored label"));
    literal(root, v("expiry"));
    literal(root, v("outbound purpose"));
    await user.click(screen.getByText("Exact data sent"));
    literal(root, v("outbound payload"));
    inert({ exportTool: 1, revealOutput: 1, startShow: 1 });
    await user.click(button("Settings"));
    literal(root, v("route blocker"));
    literal(root, `Unknown: ${v("model error")}`);
    literal(root, `Record a task: ${v("show error")}`);
    await user.click(button("Check again"));
    literal(root, v("settings error"));
    inert({ exportTool: 1, revealOutput: 1, startShow: 1 });
  });

  it("keeps run input, destination paths, staged output and committed receipts inert", async () => {
    const { backend, v } = fixture(payload);
    const stage = vi.spyOn(backend, "stageRun");
    const { user, root, inert } = await open(backend);
    await user.click(button(`Run now: ${v("saved tool")}`));
    await user.click(button("Choose input file"));
    literal(root, v("file name"), true);
    await user.click(button("Choose output folder"));
    literal(root, v("folder path"), true);
    fireEvent.change(screen.getByLabelText("Output name"), { target: { value: v("run output name") } });
    inputLiteral("Output name", v("run output name"));
    if (pathDisplay(v("run output name")) !== v("run output name")) literal(root, v("run output name"), true);
    inert();
    await user.click(button("Preview result"));
    expect(stage).toHaveBeenCalledWith("tool", expect.objectContaining({ displayPath: v("input path") }), expect.objectContaining({ displayPath: v("folder path") }), v("run output name"));
    const preview = screen.getByRole("region", { name: "Previewed CSV · first 8 rows" });
    literal(preview, v("run header"));
    literal(preview, v("run cell"));
    inert({ stageRun: 1 });
    await user.click(button("Create previewed CSV"));
    literal(root, v("receipt path"), true);
    inert({ stageRun: 1, commitRun: 1 });
    stage.mockResolvedValueOnce({ failure: { kind: "input_invalid", message: v("run error"), row: 1, column: v("error column") } });
    await user.click(button("Run again"));
    await user.click(button("Preview result"));
    const alert = screen.getByRole("alert");
    await user.click(alert.nextElementSibling!.querySelector("summary")!);
    literal(root, v("run error"));
    await user.click(screen.getByText("File details"));
    literal(root, `Row: 1 · Column: ${v("error column")}`);
    inert({ stageRun: 2, commitRun: 1 });
  });
});

it("opens HTTPS documentation only after reviewing the address and confirming", async () => {
  const address = "https://example.invalid/documentation";
  const { backend, v } = fixture(address);
  vi.spyOn(backend, "openExternal").mockResolvedValue();
  const { user, root, inert } = await open(backend);
  await user.click(button("Connections"));
  const catalog = screen.getByText(v("connector"), textOptions).closest("article")!;
  // Choosing the service starts sign-in; fail it quickly so the page is not busy while documentation is reviewed.
  vi.spyOn(backend, "beginConnect").mockRejectedValue(new Error("Sign-in unavailable in this test."));
  await user.click(within(catalog).getByRole("button"));
  await waitFor(() => expect(backend.beginConnect).toHaveBeenCalledOnce());
  await user.click(button("Documentation"));
  literal(screen.getByRole("region", { name: "Open documentation" }), address);
  inert({ beginConnect: 1 });
  await user.click(button("Keep here"));
  expect(screen.queryByRole("region", { name: "Open documentation" })).toBeNull();
  inert({ beginConnect: 1 });
  await user.click(button("Documentation"));
  await user.click(button("Open documentation in browser"));
  expect(backend.openExternal).toHaveBeenCalledExactlyOnceWith(address);
  expect(root.querySelectorAll("a")).toHaveLength(1);
  expect(root.querySelector("a")).toHaveAttribute("href", "#main-content");
  inert({ openExternal: 1, beginConnect: 1 });
});

async function open(backend: Backend) {
  const user = userEvent.setup();
  const clipboard = vi.spyOn(navigator.clipboard, "writeText");
  const dispatch = commands.map((name) => ({ name, spy: vi.spyOn(backend, name) }));
  const initialUrl = window.location.href;
  const { container: root } = render(<App backend={backend} />);
  await waitFor(() => expect(button("Describe a task")).toBeEnabled());
  const inert = (allowed: Partial<Record<typeof commands[number], number>> = {}) => {
    expect(root.querySelector("script, img, svg, iframe, object, embed, style, link, base, meta")).toBeNull();
    expect(root.querySelector("[style], [src], [srcset], [background], [formaction]")).toBeNull();
    const handlers = [...root.querySelectorAll("*")].flatMap((element) =>
      [...element.attributes].filter((attr) => /^on/i.test(attr.name)).map((attr) => attr.name),
    );
    expect(handlers).toEqual([]);
    for (const link of root.querySelectorAll("a")) {
      if (link.classList.contains("skip-link")) expect(link.getAttribute("href")).toBe("#main-content");
      else expect(new URL(link.href).protocol).toBe("https:");
    }
    expect(fetch).not.toHaveBeenCalled();
    expect(window.open).not.toHaveBeenCalled();
    expect(window.alert).not.toHaveBeenCalled();
    expect(clipboard).not.toHaveBeenCalled();
    expect(window.location.href).toBe(initialUrl);
    for (const { name, spy } of dispatch) {
      expect(spy, `${name} dispatched while rendering`).toHaveBeenCalledTimes(allowed[name] ?? 0);
    }
  };
  inert();
  return { user, root, inert };
}

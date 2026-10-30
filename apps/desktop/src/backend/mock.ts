import { RecordingMock } from "./recordingMock";
import connectorCatalog from "../../../../contracts/connectors.json";
import type {
  ActivityItem,
  AppInfo,
  Backend,
  BuildPhase,
  BuildState,
  ConnectionSuggestion,
  ConnectorInfo,
  ConnectorTool,
  CsvMapping,
  CsvPreview,
  FileHandle,
  ModelAccessStatus,
  ModelRoute,
  OutboundRecord,
  Receipt,
  RunFailure,
  ShowState,
  StagedRun,
  TaskDraft,
  TaskKind,
  ToolDetail,
  ToolSummary,
  Transform,
} from "./types";
import { sampleCsv } from "./sample";

const clone = <T>(value: T): T => structuredClone(value);
export interface MockBackendOptions {
  /** Test-only responses at the backend boundary, including malformed server data.
   * Omitted by the browser demo so its state machine stays unchanged. */
  testOnlyOverrides?: Partial<Backend>;
}
const delay = (ms: number) =>
  new Promise<void>((resolve) => setTimeout(resolve, ms));
const mapping = (): CsvMapping => ({
  output_columns: [
    { name: "Order", source: "order_id", transform: { type: "copy" } },
    {
      name: "Date",
      source: "created",
      transform: { type: "date", from: "mm_dd_yy", to: "iso8601" },
    },
    {
      name: "Amount",
      source: "amount",
      transform: { type: "money", currency: "USD", decimals: 2 },
    },
  ],
  on_error: "stop_and_explain",
  skip_header_rows: 0,
  formula_escape: true,
});
const access = [
  "Read the CSV you choose",
  "Create one new CSV in the folder you choose",
  "Source unchanged; existing files are never replaced",
];
const blockedReason =
  "Add a Meta API key or sign in to Muse Code to build with a model.";
export const routeLabel = (route: ModelRoute | null) =>
  route === "api_key"
    ? "Meta API key"
    : route === "muse_plan"
      ? "Muse plan"
      : route === "custom_endpoint" ? "Custom endpoint" : "Offline demo";

/** Small RFC 4180 reader. Cells remain text, including anything resembling markup. */
function parseCsv(text: string): string[][] {
  const rows: string[][] = [];
  let row: string[] = [];
  let cell = "";
  let quoted = false;
  for (let i = 0; i < text.length; i++) {
    const c = text[i];
    if (c === '"') {
      if (quoted && text[i + 1] === '"') {
        cell += '"';
        i++;
      } else quoted = !quoted;
    } else if (c === "," && !quoted) {
      row.push(cell);
      cell = "";
    } else if ((c === "\n" || c === "\r") && !quoted) {
      if (c === "\r" && text[i + 1] === "\n") i++;
      row.push(cell);
      rows.push(row);
      row = [];
      cell = "";
    } else cell += c;
  }
  if (quoted)
    throw new Error("CSV has an unclosed quoted cell. Choose a valid CSV.");
  if (cell || row.length) {
    row.push(cell);
    rows.push(row);
  }
  if (rows.length < 2 || !rows[0].length)
    throw new Error("Choose a CSV with a header and at least one data row.");
  if (new Set(rows[0]).size !== rows[0].length)
    throw new Error("CSV column names must be unique.");
  return rows;
}
function transformCell(value: string, rule: Transform): string {
  switch (rule.type) {
    case "copy":
      return value;
    case "trim":
      return value.trim();
    case "upper":
      return value.toUpperCase();
    case "lower":
      return value.toLowerCase();
    case "constant":
      return rule.value;
    case "money": {
      if (
        !Number.isInteger(rule.decimals) ||
        rule.decimals < 0 ||
        rule.decimals > 6
      )
        throw new Error("Use 0–6 decimal places.");
      const clean = value.replace(/[$,£€\s]/g, "");
      if (!/^-?\d+(\.\d+)?$/.test(clean))
        throw new Error("Amount must be a decimal number.");
      return Number(clean).toFixed(rule.decimals);
    }
    case "date": {
      const parts = value.split(/[\/-]/).map(Number);
      let year: number, month: number, day: number;
      if (rule.from === "iso8601" || rule.from === "yyyy_mm_dd")
        [year, month, day] = parts;
      else if (rule.from === "dd_mm_yyyy" || rule.from === "dd_mm_yy")
        [day, month, year] = parts;
      else [month, day, year] = parts;
      if (year < 100) year += 2000;
      const date = new Date(Date.UTC(year, month - 1, day));
      if (
        parts.length !== 3 ||
        date.getUTCFullYear() !== year ||
        date.getUTCMonth() !== month - 1 ||
        date.getUTCDate() !== day
      )
        throw new Error(
          "Date is malformed. Correct this date before running again.",
        );
      const m = String(month).padStart(2, "0"),
        d = String(day).padStart(2, "0");
      if (rule.to === "mm_dd_yy") return `${m}/${d}/${String(year).slice(-2)}`;
      if (rule.to === "mm_dd_yyyy") return `${m}/${d}/${year}`;
      if (rule.to === "dd_mm_yy") return `${d}/${m}/${String(year).slice(-2)}`;
      if (rule.to === "dd_mm_yyyy") return `${d}/${m}/${year}`;
      return `${year}-${m}-${d}`;
    }
  }
}
function apply(
  preview: CsvPreview,
  rows: string[][],
  map: CsvMapping,
): string[][] | RunFailure {
  const result: string[][] = [];
  for (const [i, row] of rows.slice(map.skip_header_rows).entries()) {
    if (row.length !== preview.columns.length)
      return {
        kind: "input_invalid",
        row: i + 1,
        column: null,
        message: "Row has a different number of cells than the header.",
      };
    const out: string[] = [];
    for (const column of map.output_columns) {
      const index = preview.columns.indexOf(column.source);
      if (index < 0 && column.transform.type !== "constant")
        return {
          kind: "input_invalid",
          row: 1,
          column: column.source,
          message:
            "Required column is missing. Choose another input or edit the rules.",
        };
      try {
        let value = transformCell(row[index] ?? "", column.transform);
        if (map.formula_escape && /^[=+@\-\t\r]/.test(value))
          value = "'" + value;
        out.push(value);
      } catch (error) {
        return {
          kind: "input_invalid",
          row: i + 1,
          column: column.source,
          message: (error as Error).message,
        };
      }
    }
    result.push(out);
  }
  return result;
}
function digest(value: unknown): string {
  let hash = 2166136261;
  for (const c of JSON.stringify(value))
    hash = Math.imul(hash ^ c.charCodeAt(0), 16777619);
  return (hash >>> 0).toString(16).padStart(8, "0").repeat(8);
}

/** Fresh, isolated in-memory product state. No credentials or files persist. */
export class MockBackend implements Backend {
  private recording = new RecordingMock(
    () => this.resolveRoute(), description => this.draftTaskNoInput(description), row => this.outbound.unshift(row),
    () => {
      if (this.resolveRoute().effectiveRoute !== "custom_endpoint") return;
      const c = this.status.customEndpoint;
      if (c.callsThisMonth >= c.monthlyCallCap) throw new Error("Skipped — monthly custom endpoint call cap reached");
      c.callsThisMonth++;
    },
  );
  recordingStatus: Backend["recordingStatus"] = () => this.recording.recordingStatus();
  confirmNormalBrowserWindow: Backend["confirmNormalBrowserWindow"] = () => this.recording.confirmNormalBrowserWindow();
  saveRecordingFrame: Backend["saveRecordingFrame"] = async () => null;
  async findConnectors(query: string) {
    return /airtable/i.test(query) ? [{ name: "com.airtable/mcp", description: "Official Airtable MCP server", url: "https://mcp.airtable.com/mcp", publisher: "airtable.com" }] : [];
  }
  setRecordingMaskLevel: Backend["setRecordingMaskLevel"] = (id, level) => this.recording.setRecordingMaskLevel(id, level);
  removeRecordingFrame: Backend["removeRecordingFrame"] = (id, seq) => this.recording.removeRecordingFrame(id, seq);
  openPrivacySettings: Backend["openPrivacySettings"] = pane => this.recording.openPrivacySettings(pane);
  requestRecordingPermission: Backend["requestRecordingPermission"] = () => this.recording.requestRecordingPermission();
  startRecording: Backend["startRecording"] = source => this.recording.startRecording(source);
  stopRecording: Backend["stopRecording"] = id => this.recording.stopRecording(id);
  listRecordingFrames: Backend["listRecordingFrames"] = id => this.recording.listRecordingFrames(id);
  previewRecording: Backend["previewRecording"] = id => this.recording.previewRecording(id);
  consentAndAnalyze: Backend["consentAndAnalyze"] = (id, hash) => this.recording.consentAndAnalyze(id, hash);
  answerRecordingQuestions: Backend["answerRecordingQuestions"] = (id, answers) => this.recording.answerRecordingQuestions(id, answers);
  approveRecordingDraft: Backend["approveRecordingDraft"] = id => this.recording.approveRecordingDraft(id);
  discardRecording: Backend["discardRecording"] = id => this.recording.discardRecording(id);
  private schedules = new Map<string, import("./types").Schedule>();
  private today: import("./types").TodayDay[] = [];
  private login = false;
  async listToday() {
    const cutoff = Date.now() - 30 * 24 * 60 * 60 * 1000;
    this.today = this.today.map(day => ({...day, entries: day.entries.filter(entry => entry.status === "needs_you" || entry.startedAtMs >= cutoff)})).filter(day => day.entries.length);
    return clone(this.today);
  }
  async dismissTodayEntry(id: string) { this.today = this.today.map(d => ({...d, entries: d.entries.filter(e=>e.id!==id)})).filter(d=>d.entries.length); }
  async listSchedules() { return clone([...this.schedules.values()].map(s=> ({...s, status: this.tools.get(s.toolId)?.versionHash === s.versionHash ? s.status : "needs_reapproval" as const}))); }
  async setSchedule(toolId: string, request: import("./types").ScheduleRequest) {
    const tool = await this.getTool(toolId);
    if (!request.approved || request.versionHash !== tool.versionHash) throw new Error("Review and approve this tool version running unattended");
    if (!tool.noInput) throw new Error("Schedules require a tool without a selected input file");
    if (!request.outputs.today && !request.outputs.notification && !request.outputs.folder) throw new Error("Choose at least one output");
    if (!/^([01]\d|2[0-3]):[0-5]\d$/.test(request.frequency.time)) throw new Error("Time must be HH:MM");
    if (request.frequency.kind === "weekly" && (request.frequency.weekday < 1 || request.frequency.weekday > 7)) throw new Error("Choose a valid weekday");
    if (request.frequency.kind === "monthly" && (request.frequency.day < 1 || request.frequency.day > 31)) throw new Error("Choose day 1–31");
    const next = new Date(); const [h,m] = request.frequency.time.split(":").map(Number); next.setHours(h,m,0,0);
    for (let i=0;i<370;i++) {
      const f=request.frequency;
      const matches=f.kind === "daily" || (f.kind === "weekly" ? ((next.getDay()+6)%7)+1===f.weekday : next.getDate()===Math.min(f.day,new Date(next.getFullYear(),next.getMonth()+1,0).getDate()));
      if (+next>Date.now() && matches) break;
      next.setDate(next.getDate()+1);
    }
    const schedule: import("./types").Schedule = {...clone(request),id:`schedule-${toolId}`,toolId,binding:tool.versionHash,folderIdentity:null,status:"enabled",approvedAtMs:Date.now(),lastDueMs:Date.now(),nextRunMs:+next};
    this.schedules.set(toolId,schedule); return clone(schedule);
  }
  async scheduleStatus(toolId: string, status: "enabled" | "paused" | "remove") {
    if(status==="remove") {this.schedules.delete(toolId);return;}
    const s=this.schedules.get(toolId); if(!s) throw new Error("Schedule not found");
    if(status==="enabled" && (s.status==="needs_reapproval" || this.tools.get(toolId)?.versionHash!==s.versionHash)) throw new Error("Schedule needs re-approval");
    s.status=status;
  }
  async loginEnabled() {return this.login;}
  async setLoginEnabled(enabled: boolean) {this.login=enabled;}
  private serial = 0;
  private tools = new Map<string, ToolDetail>();
  private files = new Map<string, { preview: CsvPreview; rows: string[][] }>();
  private drafts = new Map<string, TaskDraft>();
  private builds = new Map<string, BuildState>();
  private readApprovals = new Map<string, () => void>();
  private modelStepCounts = new Map<string, number>();
  private buildDrafts = new Map<string, TaskDraft>();
  private buildListeners = new Map<string, Set<(state: BuildState) => void>>();
  private staged = new Map<string, StagedRun>();
  private stagedData = new Map<
    string,
    { inputDigest: string; outputDigest: string }
  >();
  /** Approved fetch hosts per tool, mirroring the core's tool grants. */
  private webHosts = new Map<string, string[]>();
  private sampleInputs = new Set<string>();
  private destinations = new Set<string>();
  private receipts: Receipt[] = [];
  private activity: ActivityItem[] = [];
  private outbound: OutboundRecord[] = [];
  private pending = new Map<string, string>();
  private dismissed = new Set<string>();
  private suggestions: ConnectionSuggestion[] = [];
  private show: ShowState = { kind: "idle", accessibility: "granted" };
  private showListeners = new Set<(state: ShowState) => void>();
  private museReadyAt: number | null = null;
  private status: ModelAccessStatus = {
    customEndpoint: {baseUrl:null, host:null, model:null, displayName:null, tokenPresent:false, lastTest:null, monthlyCallCap:500, callsThisMonth:0},
    apiKey: { present: false, fingerprint: null, lastTest: null },
    muse: { kind: "not_signed_in", version: "mock" },
    selectedRoute: null,
    effectiveRoute: "offline_demo",
    blockedReason,
    egress: "cloud_allowed",
    model: "muse-spark-1.3",
    museUnverifiedOk: false,
  };
  private connectors: ConnectorInfo[] = connectorCatalog.connectors.map(
    (c) => ({
      id: c.id,
      name: c.name,
      category: c.category,
      mcpUrl: c.mcp_url,
      registration: c.auth.registration as ConnectorInfo["registration"],
      verification: c.verification as ConnectorInfo["verification"],
      docsUrl: c.auth.docs_url || null,
      clientSetup: c.auth.client_setup ?? null,
      redirectUri:
        c.auth.fixed_redirect_port == null
          ? null
          : `http://127.0.0.1:${c.auth.fixed_redirect_port}/callback`,
      state: { kind: "not_connected" },
      hasClientCredentials: false,
    }),
  );

  constructor(
    seeds: (Pick<ToolSummary, "id" | "name" | "version"> &
      Partial<ToolSummary>)[] = [],
    options: MockBackendOptions = {},
  ) {
    for (const seed of seeds)
      this.tools.set(seed.id, {
        modelStepCap:60,
        engine: "csv-mapping",
        createdAtMs: Date.now(),
        lastRunAtMs: null,
        runCount: 0,
        sample: false,
        state: "enabled",
        summary: ["Map orders for import"],
        mapping: mapping(),
        inputColumns: ["order_id", "created", "amount", "customer"],
        outputColumns: ["Order", "Date", "Amount"],
        access,
        versionHash: digest(seed),
        noInput: false,
        ...clone(seed),
      });
    Object.assign(this, options.testOnlyOverrides);
  }
  private id(prefix: string) {
    return `${prefix}-${++this.serial}`;
  }
  private log(
    kind: ActivityItem["kind"],
    title: string,
    detail: string | null = null,
  ) {
    this.activity.unshift({
      id: this.id("activity"),
      atMs: Date.now(),
      kind,
      title,
      detail,
    });
  }
  private resolveRoute() {
    const selected = this.status.selectedRoute;
    const available = (route: ModelRoute) => route === "api_key" ? this.status.apiKey.present
      : route === "muse_plan" ? this.status.muse.kind === "signed_in"
      : route === "custom_endpoint" ? this.customReady() : true;
    this.status.effectiveRoute = this.status.egress !== "cloud_allowed" ? "offline_demo"
      : selected !== null ? available(selected) ? selected : "offline_demo"
      : this.status.apiKey.present ? "api_key"
      : this.status.muse.kind === "signed_in" ? "muse_plan" : "offline_demo";
    this.status.blockedReason =
      this.status.selectedRoute === "offline_demo"
        ? null
        : this.status.egress !== "cloud_allowed"
          ? "Local only is enabled. Use cloud models in Privacy or choose Offline demo."
          : this.status.effectiveRoute === "offline_demo"
            ? blockedReason
            : null;
    this.status.model = this.status.effectiveRoute === "custom_endpoint" ? this.status.customEndpoint.model! : "muse-spark-1.3";
    return clone(this.status);
  }
  async appInfo(): Promise<AppInfo> {
    return {
      version: "0.1.0",
      platform: "browser" as const,
      mock: true,
      secretStore: "memory",
      debugBuild: false,
    };
  }
  async modelAccess() {
    return this.resolveRoute();
  }
  async enterApiKeyNative(): Promise<ModelAccessStatus> {
    throw new Error("Use the inline secure field in this browser.");
  }
  async saveApiKey(key: string) {
    if (!key.trim()) throw new Error("Enter an API key.");
    this.status.apiKey = {
      present: true,
      fingerprint: key.slice(-4),
      lastTest: null,
    };
    this.log("settings", "API key saved");
    return this.resolveRoute();
  }
  async removeApiKey() {
    this.status.apiKey = { present: false, fingerprint: null, lastTest: null };
    return this.resolveRoute();
  }
  async testApiKey() {
    if (!this.status.apiKey.present) throw new Error("Enter a key first.");
    this.status.apiKey.lastTest = { kind: "ok", checkedAtMs: Date.now() };
    return this.resolveRoute();
  }
  async openMuseSignIn() {
    this.museReadyAt = Date.now() + 700;
  }
  async refreshMuseStatus() {
    if (this.museReadyAt !== null && Date.now() >= this.museReadyAt)
      this.status.muse = {
        kind: "signed_in",
        version: "mock",
        planSummary: "Muse plan · allowance available (demo)",
      };
    return this.resolveRoute();
  }
  private customReady() { const c=this.status.customEndpoint; return !!(c.baseUrl && c.model && c.tokenPresent && c.lastTest?.status === "ok"); }
  async saveCustomEndpoint(baseUrl: string, model: string) {
    let u: URL; try { u=new URL(baseUrl); } catch { throw new Error("Use an HTTPS base URL."); }
    if(u.protocol!=="https:" || u.username || u.password || baseUrl.includes("?") || baseUrl.includes("#") || !model.trim()) throw new Error("Use HTTPS without userinfo, query or fragment, and enter a model.");
    const c=this.status.customEndpoint;
    if(c.baseUrl!==u.href || c.model!==model.trim()) c.lastTest=null;
    this.recording.invalidateConsent();
    Object.assign(c,{baseUrl:u.href,host:u.hostname,model:model.trim()}); return this.resolveRoute();
  }
  async saveCustomEndpointToken() { this.recording.invalidateConsent(); this.status.customEndpoint.tokenPresent=true; this.status.customEndpoint.lastTest=null; return this.resolveRoute(); }
  async setCustomEndpointOptions(displayName: string | null, monthlyCallCap: number) {
    if(!Number.isInteger(monthlyCallCap) || monthlyCallCap<0) throw new Error("Enter a nonnegative whole call cap.");
    Object.assign(this.status.customEndpoint,{displayName,monthlyCallCap}); return this.resolveRoute();
  }
  async testCustomEndpoint() {
    const c=this.status.customEndpoint;
    if(this.status.egress!=="cloud_allowed") throw new Error("Local only blocks endpoint tests");
    if(!c.baseUrl || !c.model || !c.tokenPresent) throw new Error("Set URL, model and token first.");
    if(c.callsThisMonth>=c.monthlyCallCap) throw new Error("Skipped — monthly custom endpoint call cap reached");
    c.callsThisMonth++; c.lastTest={status:"ok",latencyMs:7}; return this.resolveRoute();
  }
  async removeCustomEndpoint() { this.recording.invalidateConsent(); this.status.customEndpoint={baseUrl:null,host:null,model:null,displayName:null,tokenPresent:false,lastTest:null,monthlyCallCap:500,callsThisMonth:this.status.customEndpoint.callsThisMonth}; return this.resolveRoute(); }
  async selectRoute(route: ModelRoute) {
    if(route==="custom_endpoint" && !this.customReady()) throw new Error("Configure and test the custom endpoint first.");
    if (route === "api_key" && !this.status.apiKey.present)
      throw new Error("Enter a key first.");
    if (route === "muse_plan" && this.status.muse.kind !== "signed_in")
      throw new Error("Sign in to Muse Code first.");
    this.status.selectedRoute = route;
    return this.resolveRoute();
  }
  async setMuseUnverifiedOk(ok: boolean) {
    this.status.museUnverifiedOk = ok;
    return this.resolveRoute();
  }
  async setLocalOnly(localOnly: boolean) {
    this.recording.invalidateConsent();
    if (localOnly) {
      this.status.egress = "stopping";
      for (const b of this.builds.values())
        if (
          b.route !== "offline_demo" &&
          !["ready", "failed", "stopped"].includes(b.phase)
        )
          await this.stopBuild(b.buildId);
      await delay(150);
    }
    this.status.egress = localOnly ? "local_only" : "cloud_allowed";
    return this.resolveRoute();
  }
  /** Also useful for tests supplying malicious or malformed CSVs. */
  importCsv(name: string, text: string): CsvPreview {
    const [columns, ...rows] = parseCsv(text);
    const handle = {
      handleId: this.id("file"),
      displayName: name,
      displayPath: name,
    };
    const preview = {
      handle,
      columns,
      rows: rows
        .slice(0, 8)
        .map((row) => row.map((cell) => cell.slice(0, 200))),
      rowCount: rows.length,
      byteSize: new TextEncoder().encode(text).length,
    };
    this.files.set(handle.handleId, { preview, rows });
    return clone(preview);
  }
  importText(name: string, text: string): CsvPreview {
    if (new TextEncoder().encode(text).length > 10_000_000) throw new Error("Input exceeds 10 MB.");
    const handle = {handleId: this.id("file"), displayName: name, displayPath: name};
    const preview = {handle, columns: [], rows: [], rowCount: text.split("\n").length, byteSize: new TextEncoder().encode(text).length, text: text.split("\n").slice(0, 8).join("\n")};
    this.files.set(handle.handleId, {preview, rows: text.split("\n").map(line => [line])});
    return clone(preview);
  }
  async loadSampleCsv() {
    const preview = this.importCsv("orders.csv", sampleCsv);
    this.sampleInputs.add(preview.handle.handleId);
    return preview;
  }
  async pickCsv(): Promise<CsvPreview | null> {
    return new Promise((resolve, reject) => {
      const input = document.createElement("input");
      input.type = "file";
      input.accept = ".csv,.tsv,.txt,.md,.json,.log,.xml,.html";
      input.onchange = () => {
        const file = input.files?.[0];
        if (!file) {
          resolve(null);
          return;
        }
        void file.text().then((text) => {
          try {
            resolve(file.name.toLowerCase().endsWith(".csv") ? this.importCsv(file.name, text) : this.importText(file.name, text));
          } catch (error) {
            reject(error);
          }
        }, reject);
      };
      input.oncancel = () => resolve(null);
      input.click();
    });
  }
  private draft(id: string) {
    const draft = this.drafts.get(id);
    if (!draft)
      throw new Error("Task draft is no longer available. Describe it again.");
    return draft;
  }
  private validate(draft: TaskDraft) {
    draft.blockers = [];
    if (draft.kind === "program") {
      if (!/^[^/\\:]+\.(csv|tsv|txt|md|json)$/i.test(draft.outputName)) draft.blockers.push("Choose a .csv, .tsv, .txt, .md or .json output name.");
      draft.readyToBuild = !draft.blockers.length;
      return clone(draft);
    }
    if (draft.questions.some((q) => !q.answer))
      draft.blockers.push("Choose the date meaning to continue.");
    if (!/^[^/\\]+\.csv$/i.test(draft.outputName))
      draft.blockers.push(
        "Choose an output file name ending in .csv, without folder separators.",
      );
    if (!draft.mapping.output_columns.length)
      draft.blockers.push("Add at least one output column.");
    const names = draft.mapping.output_columns.map((c) => c.name.trim());
    if (names.some((name) => !name) || new Set(names).size !== names.length)
      draft.blockers.push("Give every output column a unique name.");
    for (const c of draft.mapping.output_columns)
      if (
        !draft.input.columns.includes(c.source) &&
        c.transform.type !== "constant"
      )
        draft.blockers.push(`Choose a source column for ${c.name}.`);
    draft.readyToBuild = draft.blockers.length === 0;
    return clone(draft);
  }
  async draftTask(input: FileHandle, description: string) {
    const file = this.files.get(input.handleId);
    if (!file) throw new Error("Choose your input CSV again.");
    const mentionedConnectors = this.connectors
      .filter(
        (c) =>
          c.id !== "custom" &&
          new RegExp(`\\b${c.name}\\b`, "i").test(description),
      )
      .map((c) => c.id);
    const kind = file.preview.text !== undefined || /extract|filter|containing|sort|group|json|dedup|duplicat|blank rows|empty rows|calculat|multiply|divide|subtract|percent|tax|replace|regex|search|find|pivot|split/i.test(description) ? "program" : "csv-mapping";
    const draft: TaskDraft = {
      kind, exampleOutput: null, noInput: false,
      draftId: this.id("draft"),
      revision: 1,
      goal: description,
      input: clone(file.preview),
      summary: [
        "You described this task; recurrence unmeasured.",
        `Map ${file.preview.rowCount} rows to Order, Date, Amount.`,
        "Convert dates to YYYY-MM-DD and amounts to decimal USD; stop and explain malformed values.",
        "Create one new CSV; leave the source unchanged.",
      ],
      mapping: mapping(),
      questions: [
        {
          id: "date-format",
          column: "created",
          prompt: "Does 03/04/26 mean March 4 or April 3?",
          options: [
            { value: "mm_dd_yy", label: "March 4 (MM/DD/YY)" },
            { value: "dd_mm_yy", label: "April 3 (DD/MM/YY)" },
          ],
          answer: null,
        },
      ],
      mentionedConnectors,
      outputName: "orders_ready.csv",
      readyToBuild: false,
      blockers: [],
    };
    if (kind === "program") {
      draft.questions = [];
      draft.outputName = "errors.txt";
      draft.summary = [description, "Program (sandboxed): create one output; source unchanged."];
    }
    this.drafts.set(draft.draftId, draft);
    return this.validate(draft);
  }
  /** Drafts a program with no input file; the mock weather build declares one web source. */
  async draftTaskNoInput(description: string) {
    const handle = {
      handleId: `no-input-${this.serial + 1}`,
      displayName: "No input file",
      displayPath: "",
    };
    const draft: TaskDraft = {
      kind: "program",
      exampleOutput: null,
      noInput: true,
      draftId: this.id("draft"),
      revision: 1,
      goal: description,
      input: {
        handle,
        columns: [],
        rows: [],
        rowCount: 0,
        byteSize: 0,
      },
      summary: [
        description,
        "Program (sandboxed): no input file; read approved web sources and create one output.",
        "Create weather.txt; the program reads fetched web data through macroscope.source.",
      ],
      mapping: mapping(),
      questions: [],
      mentionedConnectors: [],
      outputName: "weather.txt",
      readyToBuild: true,
      blockers: [],
    };
    this.drafts.set(draft.draftId, draft);
    return this.validate(draft);
  }
  async answerQuestion(draftId: string, questionId: string, answer: string) {
    const draft = this.draft(draftId);
    const q = draft.questions.find((q) => q.id === questionId);
    if (!q || !q.options.some((o) => o.value === answer))
      throw new Error("Choose one of the offered answers.");
    q.answer = answer;
    for (const c of draft.mapping.output_columns)
      if (c.source === q.column && c.transform.type === "date")
        c.transform.from = answer as "mm_dd_yy" | "dd_mm_yy";
    draft.revision++;
    return this.validate(draft);
  }
  async updateMapping(draftId: string, map: CsvMapping, outputName: string) {
    const draft = this.draft(draftId);
    draft.mapping = clone(map);
    draft.outputName = outputName;
    draft.revision++;
    return this.validate(draft);
  }
  async selectTaskConnections(draftId: string, connectorIds: string[]) {
    const draft = this.draft(draftId);
    draft.requiredConnections = [];
    for (const id of connectorIds) {
      const connector = (await this.listConnectors()).find(c => c.id === id && c.state.kind === "connected");
      if (!connector) throw new Error("Connect this service before using it.");
      draft.requiredConnections.push({connectorId:id, name:connector.name, resource:connector.mcpUrl, tools:await this.previewConnectorTools(id)});
    }
    if (connectorIds.length) { draft.kind = "program"; draft.questions = []; }
    draft.revision++;
    return this.validate(draft);
  }
  async updateTask(draftId: string, kind: TaskKind, outputName: string, exampleOutput: string | null) {
    const draft = this.draft(draftId);
    draft.kind = kind; draft.outputName = outputName; draft.exampleOutput = exampleOutput;
    if (kind === "program") draft.questions = [];
    draft.revision++;
    return this.validate(draft);
  }
  async outboundPreview(draftId: string): Promise<import("./types").OutboundPreview> {
    const draft = this.draft(draftId),
      status = this.resolveRoute(),
      route = status.effectiveRoute ?? "offline_demo";
    const recipient = route === "custom_endpoint" ? status.customEndpoint.host! : route === "api_key" ? "Meta API (api.meta.ai)" : route === "muse_plan" ? "Muse Code with your Muse plan" : "Nothing leaves this Mac";
    const includes: import("./types").OutboundSummary["includes"] = route === "offline_demo" ? [] : [
      ...(draft.kind === "program" ? [{label:"Your task description",detail:draft.goal,real:true}, ...(draft.exampleOutput ? [{label:"Your example result",detail:draft.exampleOutput,real:true}] : [])] : [{label:"Your column names and confirmed rules",real:true}]),
      ...(!draft.noInput ? [{label:"A made-up sample of your data's shape (no real rows)",real:false}] : []),
      {label:"Standard instructions for writing the tool",real:false},
    ];
    const laterRuns = draft.noInput ? ["Reads approved websites or connections each run; you choose how often in Schedule"] : ["Reads the file you select each time you run it"];
    for (const match of draft.goal.matchAll(/https?:\/\/[^\s]+/g)) {
      try { laterRuns.unshift(`Requested site: ${new URL(match[0]).hostname} on each run; exact fetches need approval after the build`); } catch { /* Plain text is not necessarily a URL. */ }
    }
    for (const c of draft.requiredConnections ?? []) {
      if (route !== "offline_demo" && draft.kind === "program") includes.push({label:`Available actions and read instructions for ${c.name}`,real:false});
      laterRuns.push(`May read from ${c.name}; exact reads and model instructions need your approval after the build`);
    }
    laterRuns.push("Creates a result for your review; external actions need your approval");
    return {
      summary: {recipient,model:route === "offline_demo" ? "No model" : status.model,includes,laterRuns,cost:route === "offline_demo" ? "Free; no model calls" : route === "api_key" ? "Estimated cost: $0.02 · Budget: $2.00" : route === "muse_plan" ? "Counts against your Muse plan" : `Price unknown · ${status.customEndpoint.callsThisMonth} of ${status.customEndpoint.monthlyCallCap} monthly calls used`},
      draftId,
      route,
      webHosts: [],
      model: route === "offline_demo" ? "No model" : status.model,
      destination:
        route === "custom_endpoint" ? status.customEndpoint.host! : route === "api_key"
          ? "Meta API (api.meta.ai)"
          : route === "muse_plan"
            ? "Muse Code with your Muse plan"
            : "Nothing leaves this Mac",
      payloadText:
        route === "offline_demo"
          ? "Nothing leaves this Mac. The offline demo maps columns without a model."
          : draft.kind === "program" ? JSON.stringify({description: draft.goal, syntheticTextShape: draft.noInput ? "" : "2123-45-67 HJKLM inventedvalue", exampleRealData: draft.exampleOutput, outputName: draft.outputName}) : JSON.stringify(
              {
                task: "Create a CSV mapping using these confirmed rules.",
                mapping: draft.mapping,
                syntheticRows: [
                  {
                    order_id: "SYN-001",
                    created: "03/04/26",
                    amount: "$12.34",
                  },
                ],
                onError: "stop_and_explain",
              },
              null,
              2,
            ),
      containsRealRows: route !== "offline_demo" && draft.kind === "program" && !!draft.exampleOutput,
      estimatedCostMicros:
        route === "muse_plan" || route === "custom_endpoint" ? null : route === "offline_demo" ? 0 : 20000,
      budgetMicros: 2000000,
    };
  }
  private emitBuild(build: BuildState, phase: BuildPhase, message: string) {
    build.phase = phase;
    build.events.push({
      buildId: build.buildId,
      seq: build.events.length + 1,
      phase,
      message,
      atMs: Date.now(),
    });
    this.buildListeners.get(build.buildId)?.forEach((cb) => cb(clone(build)));
  }
  async startBuild(draftId: string, consent: { outboundReviewed: boolean }) {
    const draft = clone(this.draft(draftId));
    if(this.status.selectedRoute === "custom_endpoint" && this.resolveRoute().effectiveRoute !== "custom_endpoint") throw new Error("Custom endpoint is unavailable or blocked by Local only. Check Settings.");
    if (draft.kind === "program" && (this.resolveRoute().effectiveRoute ?? "offline_demo") === "offline_demo") throw new Error("Writing a program needs a model: sign in to Muse or add a key");
    if (!this.validate(draft).readyToBuild)
      throw new Error(draft.blockers.join(" "));
    if (
      this.resolveRoute().effectiveRoute !== "offline_demo" &&
      !consent.outboundReviewed
    )
      throw new Error("Review what will be sent before building.");
    if (
      [...this.builds.values()].some(
        (b) => !["ready", "failed", "stopped"].includes(b.phase),
      )
    )
      throw new Error("Stop the current build before starting another.");
    const preview = await this.outboundPreview(draftId),
      status = this.resolveRoute();
    if(preview.route === "custom_endpoint") {
      const c=this.status.customEndpoint;
      if(c.callsThisMonth>=c.monthlyCallCap) throw new Error("Skipped — monthly custom endpoint call cap reached");
      c.callsThisMonth++;
    }
    const build: BuildState = {
      buildId: this.id("build"),
      draftId,
      route: preview.route,
      phase: "queued",
      events: [],
      candidate: null,
      checks: [],
      comparison: null,
      spentMicros: preview.estimatedCostMicros,
      failure: null,
      routeLabel: preview.route === "custom_endpoint" ? `Custom endpoint · ${status.customEndpoint.model?.split("/").pop()} · ${status.customEndpoint.host}` : routeLabel(preview.route),
    };
    this.builds.set(build.buildId, build);
    this.buildDrafts.set(build.buildId, clone(draft));
    if (status.effectiveRoute === null) {
      const reason = status.blockedReason ?? "No build route is available.";
      build.failure = {
        kind: "no_model_access",
        message: reason,
      };
      this.emitBuild(build, "failed", reason);
      return clone(build);
    }
    if (preview.route !== "offline_demo")
      this.outbound.unshift({
        id: this.id("outbound"),
        atMs: Date.now(),
        purpose: draft.noInput ? "Build web tool" : "Build CSV tool",
        destination: preview.destination,
        route: preview.route,
        syntheticOnly: !draft.exampleOutput,
        costMicros: preview.estimatedCostMicros,
        payloadText: preview.payloadText,
      });
    this.emitBuild(build, "queued", "Build queued");
    void (async () => {
      for (const [phase, message] of [
        ["writing", "Writing the confirmed mapping"],
        ["candidate", "Candidate received"],
        ["checking", "Checking examples and source integrity"],
        ["ready", "Ready to compare"],
      ] as const) {
        await delay(500);
        if (build.phase === "stopped" || build.phase === "failed") return;
        if (phase === "candidate") {
          build.candidate = clone(draft.mapping);
          const connection=draft.noInput?draft.requiredConnections?.find(c=>c.tools.some(t=>t.readOnly===true && t.destructive!==true)):undefined;
          if(connection) {
            const tool=connection.tools.find(t=>t.readOnly===true && t.destructive!==true)!;
            const gmail=connection.connectorId==="gmail";
            const source={name:gmail?"bills":"events",connectorId:connection.connectorId,toolName:tool.name,argsJson:JSON.stringify(gmail?{query:"after:{prev_month_start} before:{month_start} bills"}:{start:"{today_start}",end:"{today_end}"})};
            build.program={summary:draft.goal,output_name:draft.outputName,webSources:[],connectionSources:[source],modelStep:{purpose:gmail?"Gather the bills info":"Write the summary",prompt:"Summarize these events"},code:`function run(){const a=macroscope.model.ask("Summarize these events",JSON.parse(macroscope.source(${JSON.stringify(source.name)})),{type:"object",properties:{summary:{type:"string"}},required:["summary"],additionalProperties:false});macroscope.output(${JSON.stringify(draft.outputName)},a.summary+"\\n");}`};
            const approval=new Promise<void>(resolve=>this.readApprovals.set(build.buildId,resolve));
            this.emitBuild(build,"candidate","Review the connection reads and model step");
            await approval;
            if(this.builds.get(build.buildId)?.phase==="stopped") return;
            continue;
          }
        }
        if (phase === "ready") {
          if (draft.noInput && build.program?.connectionSources?.length) {
            const output="Demo summary: review the events and bills.\n";
            build.checks=[{id:"deterministic",label:"Two fresh runs produce identical bytes",passed:true,detail:null}];
            build.comparison={inputColumns:[],inputRows:[],outputColumns:[],outputRows:[],inputText:"No input file",outputText:output.trimEnd(),rowsIn:0,rowsOut:1};
            this.emitBuild(build,"ready","Ready to compare");
            return;
          }
          if (draft.noInput) {
            // Simulated weather tool: one declared web source, fixed summary output.
            const output = "Auckland: fine, high 21C, low 14C.\n";
            build.program = {
              summary: "Summarize the Auckland forecast",
              output_name: draft.outputName,
              code: 'function run() { const api = macroscope.source("forecast"); macroscope.output(' + JSON.stringify(draft.outputName) + ', "Auckland: fine, high 21C, low 14C.\\n"); }',
              webSources: [{ name: "forecast", url: "https://api.open-meteo.com/v1/forecast?latitude=-36.85&longitude=174.76&daily=temperature_2m_max&start_date={today}&end_date={today}" }],
            };
            const matches = !draft.exampleOutput || output.includes(draft.exampleOutput.trim());
            build.checks = [
              { id: "web_sources", label: "Web sources fetched successfully", passed: true, detail: "api.open-meteo.com" },
              { id: "example", label: "Every example line appears in order", passed: matches, detail: null },
              { id: "deterministic", label: "Two fresh runs produce identical bytes", passed: true, detail: null },
            ];
            build.comparison = { inputColumns: [], inputRows: [], outputColumns: [], outputRows: [], inputText: "No input file", outputText: output.trimEnd(), rowsIn: 0, rowsOut: 1 };
            if (!matches) { build.failure = { kind: "checks", message: "Example lines did not appear in order." }; this.emitBuild(build, "failed", build.failure.message); return; }
            this.emitBuild(build, "ready", "Ready to compare");
            this.log("build", "Program checks passed");
            return;
          }
          const file = this.files.get(draft.input.handle.handleId)!;
          if (draft.kind === "program") {
            // Fixed mock fixture; generated code is never evaluated in the renderer.
            const output = file.rows.map(row => row.join(",")).filter(line => line.includes("ERROR")).join("\n") + "\n";
            build.program = {summary: "Extract ERROR lines", output_name: draft.outputName, code: 'function run() { const text = macroscope.input("input_csv"); macroscope.output(' + JSON.stringify(draft.outputName) + ', text.split("\\n").filter(line => line.includes("ERROR")).join("\\n") + "\\n"); }', webSources: []};
            let cursor = 0;
            const lines = output.split("\n");
            const matches = !draft.exampleOutput || draft.exampleOutput.split("\n").every(line => { const i = lines.indexOf(line, cursor); cursor = i + 1; return i >= 0; });
            build.checks = [{id:"example", label:"Every example line appears in order", passed:matches, detail:null}, {id:"deterministic",label:"Two fresh runs produce identical bytes",passed:true,detail:null}];
            build.comparison = {inputColumns:[],inputRows:[],outputColumns:[],outputRows:[],inputText:file.preview.text ?? file.rows.map(row => row.join(",")).join("\n"),outputText:output,rowsIn:file.rows.length,rowsOut:output.trimEnd().split("\n").length};
            if (!matches) { build.failure = {kind:"checks",message:"Example lines did not appear in order."}; this.emitBuild(build,"failed",build.failure.message); return; }
            this.emitBuild(build, "ready", "Ready to compare");
            this.log("build", "Program checks passed");
            return;
          }
          const beforeDigest = digest(file.rows);
          const output = apply(file.preview, file.rows, draft.mapping);
          if (!Array.isArray(output)) {
            build.failure = {
              kind: "checks",
              message: `Row ${output.kind === "input_invalid" ? output.row : "?"}: ${output.message}`,
            };
            this.emitBuild(build, "failed", build.failure.message);
            return;
          }
          const malformed = clone(file.rows);
          const dateCol = draft.mapping.output_columns.find(
            (c) => c.transform.type === "date",
          );
          if (dateCol)
            malformed[0][file.preview.columns.indexOf(dateCol.source)] =
              "99/99/26";
          const badDate = dateCol
            ? !Array.isArray(apply(file.preview, malformed, draft.mapping))
            : true;
          const dateIndexes = draft.mapping.output_columns.flatMap((c, i) =>
            c.transform.type === "date" ? [i] : [],
          );
          const moneyIndexes = draft.mapping.output_columns.flatMap((c, i) =>
            c.transform.type === "money" ? [i] : [],
          );
          const datesValid = output.every((row) =>
            dateIndexes.every((i) =>
              /^\d{4}-\d{2}-\d{2}$|^\d{2}\/\d{2}\/\d{2,4}$/.test(row[i]),
            ),
          );
          const amountsValid = output.every((row) =>
            moneyIndexes.every((i) => /^'?[-]?\d+(\.\d+)?$/.test(row[i])),
          );
          build.checks = [
            [
              "rows",
              "Row count preserved",
              output.length === file.rows.length,
              `${file.rows.length} → ${output.length}`,
            ],
            [
              "dates",
              "Dates converted",
              datesValid,
              "Confirmed date format applied",
            ],
            [
              "amounts",
              "Amounts are decimals",
              amountsValid,
              "Confirmed decimal rule applied",
            ],
            [
              "malformed",
              "Malformed date stops the run",
              badDate,
              "Invalid date rejected",
            ],
            [
              "source",
              "Source unchanged",
              beforeDigest === digest(file.rows),
              "Selected input is read only",
            ],
          ].map(([id, label, passed, detail]) => ({
            id: String(id),
            label: String(label),
            passed: Boolean(passed),
            detail: String(detail),
          }));
          build.comparison = {
            inputColumns: file.preview.columns,
            inputRows: file.preview.rows,
            outputColumns: draft.mapping.output_columns.map((c) => c.name),
            outputRows: output.slice(0, 8),
            rowsIn: file.rows.length,
            rowsOut: output.length,
          };
          if (build.checks.some((c) => !c.passed)) {
            build.failure = {
              kind: "checks",
              message: "Checks failed. Edit the rules and build again.",
            };
            this.emitBuild(build, "failed", build.failure.message);
            return;
          }
          this.log("build", "CSV tool ready to compare", build.routeLabel);
        }
        this.emitBuild(build, phase, message);
      }
    })();
    return clone(build);
  }
  async buildTryInput(buildId: string, input: FileHandle) {
    const build = this.builds.get(buildId);
    if (!build || build.phase !== "ready" || !build.candidate)
      throw new Error("Only a ready candidate can check another sample.");
    const reviewed = this.buildDrafts.get(buildId)!;
    if (this.drafts.get(reviewed.draftId)?.revision !== reviewed.revision)
      throw new Error("The task changed after this build. Build it again.");
    const file = this.files.get(input.handleId);
    if (!file) throw new Error("Choose a CSV first.");
    const result = apply(file.preview, file.rows, build.candidate);
    if (!Array.isArray(result)) {
      const detail =
        result.kind === "input_invalid"
          ? `Row ${result.row}, column ${result.column}: ${result.message}`
          : result.message;
      build.checks = [
        {
          id: "input",
          label: "New sample follows the rules",
          passed: false,
          detail,
        },
      ];
      build.comparison = null;
      build.failure = { kind: "checks", message: detail };
      this.emitBuild(build, "failed", detail);
    } else {
      build.comparison = {
        inputColumns: file.preview.columns,
        inputRows: file.preview.rows,
        outputColumns: build.candidate.output_columns.map((c) => c.name),
        outputRows: result.slice(0, 8),
        rowsIn: file.rows.length,
        rowsOut: result.length,
      };
      build.checks = [
        {
          id: "input",
          label: "New sample follows the rules",
          passed: true,
          detail: `${file.rows.length} → ${result.length}`,
        },
      ];
      build.failure = null;
      this.emitBuild(build, "ready", "New sample checks passed");
    }
    return clone(build);
  }
  async buildState(buildId: string) {
    const build = this.builds.get(buildId);
    if (!build) throw new Error("Build unavailable.");
    return clone(build);
  }
  async onBuildEvent(buildId: string, cb: (state: BuildState) => void) {
    const listeners = this.buildListeners.get(buildId) ?? new Set();
    listeners.add(cb);
    this.buildListeners.set(buildId, listeners);
    return () => {
      listeners.delete(cb);
    };
  }
  async stopBuild(buildId: string) {
    const build = this.builds.get(buildId);
    if (!build) throw new Error("Build unavailable.");
    if (!["ready", "failed", "stopped"].includes(build.phase)) {
      this.emitBuild(build, "stopped", "Build stopped before saving");
      this.log("build", "Build stopped");
      this.readApprovals.get(buildId)?.();
      this.readApprovals.delete(buildId);
    }
    return clone(build);
  }
  async approveBuildSources(buildId: string, program: import("./types").Program) {
    const build=this.builds.get(buildId);
    if(!build || build.phase!=="candidate" || JSON.stringify(build.program)!==JSON.stringify(program)) throw new Error("Candidate changed; review again");
    if(this.status.egress!=="cloud_allowed") throw new Error("Local only blocks connection reads and model steps");
    this.readApprovals.get(buildId)?.();
    this.readApprovals.delete(buildId);
    this.emitBuild(build,"checking","Checking approved sources");
    return clone(build);
  }
  async setModelStepCap(toolId:string,cap:number) {
    const tool=this.tools.get(toolId);
    if(!tool || !Number.isInteger(cap) || cap<0 || cap>10000) throw new Error("Invalid monthly cap");
    tool.modelStepCap=cap;
    return clone(tool);
  }
  async saveTool(buildId: string, name: string) {
    const build = await this.buildState(buildId);
    if (build.phase !== "ready" || !build.candidate)
      throw new Error("Checks must pass before saving.");
    if (!name.trim()) throw new Error("Give your tool a name.");
    const draft = this.buildDrafts.get(buildId)!;
    if (this.drafts.get(draft.draftId)?.revision !== draft.revision)
      throw new Error("The task changed after this build. Build it again.");
    const hosts = (build.program?.webSources ?? []).map((s) => new URL(s.url.replace("{today}", "2026-01-01")).hostname);
    const tool: ToolDetail = {
      modelStepCap:60,
      id: this.id("tool"),
      name: name.trim(),
      version: 1,
      engine: build.program ? "js-quickjs" : "csv-mapping",
      program: build.program,
      createdAtMs: Date.now(),
      lastRunAtMs: null,
      runCount: 0,
      sample: this.sampleInputs.has(draft.input.handle.handleId),
      state: "enabled",
      summary: clone(draft.summary),
      mapping: build.candidate,
      inputColumns: draft.input.columns,
      outputColumns: build.candidate.output_columns.map((c) => c.name),
      access: draft.noInput
        ? ["No input file; reads approved web sources", "Create one new output in the folder you choose", "Never overwrites a file", ...hosts.map((h) => `Fetches from ${h}`)]
        : clone(access),
      versionHash: digest({mapping:build.candidate, program:build.program}),
      noInput: draft.noInput,
    };
    this.webHosts.set(tool.id, hosts);
    this.tools.set(tool.id, tool);
    this.log("build", `Saved ${tool.name}`);
    return clone(tool);
  }
  async listTools() {
    return clone([...this.tools.values()]);
  }
  async getTool(toolId: string) {
    const tool = this.tools.get(toolId);
    if (!tool) throw new Error("Tool unavailable. Return to Tools.");
    return clone(tool);
  }
  async installSampleTool() {
    const input = await this.loadSampleCsv();
    const draft = await this.draftTask(input.handle, "Sample orders cleanup");
    await this.answerQuestion(draft.draftId, "date-format", "mm_dd_yy");
    const tool: ToolDetail = {
      modelStepCap:60,
      id: this.id("tool"),
      name: "Clean import CSV",
      version: 1,
      engine: "csv-mapping",
      createdAtMs: Date.now(),
      lastRunAtMs: null,
      runCount: 0,
      sample: true,
      state: "enabled",
      summary: draft.summary,
      mapping: mapping(),
      inputColumns: input.columns,
      outputColumns: ["Order", "Date", "Amount"],
      access: clone(access),
      versionHash: digest(mapping()),
      noInput: false,
    };
    this.tools.set(tool.id, tool);
    return clone(tool);
  }
  private exports = new Set<string>();
  private grantRevisions = new Map<string, number>();
  private runRevisions = new Map<string, number>();
  async exportTool(toolId: string) {
    const tool = await this.getTool(toolId);
    const path = `/Demo/Exports/${tool.name.replace(/[\/\\:\u0000-\u001f\u007f]/g, "_")}-v${tool.version}`;
    if (this.exports.has(path))
      throw new Error("Export folder already exists.");
    this.exports.add(path);
    this.log("settings", `Exported tool “${tool.name}”`);
    return path;
  }
  async revokeTool(toolId: string) {
    const tool = this.tools.get(toolId);
    if (!tool) throw new Error("Tool unavailable.");
    tool.state = "paused";
    this.log("settings", `Paused tool “${tool.name}”`);
    return clone(tool);
  }
  async restoreTool(toolId: string) {
    const tool = this.tools.get(toolId);
    if (!tool) throw new Error("Tool unavailable.");
    if (tool.state === "paused")
      this.grantRevisions.set(
        toolId,
        (this.grantRevisions.get(toolId) ?? 1) + 1,
      );
    tool.state = "enabled";
    return clone(tool);
  }
  async deleteTool(toolId: string) {
    this.tools.delete(toolId);
    this.webHosts.delete(toolId);
  }
  async pickOutputDir() {
    return {
      handleId: "folder-reports",
      displayName: "Reports (demo folder)",
      displayPath: "/Demo/Reports",
    };
  }
  async stageRun(
    toolId: string,
    input: FileHandle,
    outputDir: FileHandle,
    outputName: string,
  ) {
    const tool = await this.getTool(toolId),
      file = this.files.get(input.handleId);
    if (tool.state === "paused")
      return {
        failure: {
          kind: "revoked",
          message: "This tool is paused.",
        } as RunFailure,
      };
    if (tool.noInput)
      return {
        failure: {
          kind: "input_invalid",
          row: null,
          column: null,
          message: "This tool has no input file. Stage it without choosing a file.",
        } as RunFailure,
      };
    if (!file)
      return {
        failure: {
          kind: "input_invalid",
          row: null,
          column: null,
          message: "Choose an input CSV.",
        } as RunFailure,
      };
    if (!(tool.program ? /^[^/\\:]+\.(csv|tsv|txt|md|json)$/i : /^[^/\\]+\.csv$/i).test(outputName))
      return {
        failure: {
          kind: "input_invalid",
          row: null,
          column: null,
          message: "Choose a CSV file name without folder separators.",
        } as RunFailure,
      };
    const programText = tool.program ? file.rows.map(row => row.join(",")).filter(line => line.includes("ERROR")).join("\n") + "\n" : undefined;
    const output = programText !== undefined ? programText.split("\n").map(line => [line]) : apply(file.preview, file.rows, tool.mapping);
    if (!Array.isArray(output)) return { failure: output };
    const run: StagedRun = {
      previewText: programText,
      runId: this.id("run"),
      toolId,
      input: clone(input),
      outputDir: clone(outputDir),
      outputName,
      planHash: digest({
        tool: tool.versionHash,
        input: file.rows,
        output,
        outputDir,
        outputName,
      }),
      rowsIn: file.rows.length,
      rowsOut: output.length,
      outputColumns: tool.outputColumns,
      previewRows: output.slice(0, 8),
      destinationExists: this.destinations.has(
        `${outputDir.displayPath}/${outputName}`,
      ),
    };
    this.runRevisions.set(run.runId, this.grantRevisions.get(toolId) ?? 1);
    this.staged.set(run.runId, clone(run));
    this.stagedData.set(run.runId, {
      inputDigest: digest(file.rows),
      outputDigest: digest({ columns: tool.outputColumns, rows: output }),
    });
    return run;
  }
  /** Stages a run for a tool with no input file; records the simulated fetch. */
  async stageRunNoInput(toolId: string, outputDir: FileHandle, outputName: string) {
    const tool = await this.getTool(toolId);
    if (tool.state === "paused")
      return {
        failure: {
          kind: "revoked",
          message: "This tool is paused.",
        } as RunFailure,
      };
    if (!tool.noInput)
      return {
        failure: {
          kind: "input_invalid",
          row: null,
          column: null,
          message: "Choose an input file to run this tool.",
        } as RunFailure,
      };
    if (!/^[^/\\:]+\.(csv|tsv|txt|md|json)$/i.test(outputName))
      return {
        failure: {
          kind: "input_invalid",
          row: null,
          column: null,
          message: "Choose an output name without folder separators.",
        } as RunFailure,
      };
    if (this.status.egress !== "cloud_allowed")
      return {
        failure: {
          kind: "internal",
          message: "Local only is on: web sources cannot be fetched. Turn off Local only to run this tool.",
        } as RunFailure,
      };
    const hosts = this.webHosts.get(toolId) ?? [];
    const countKey=`${toolId}:${new Date().toISOString().slice(0,7)}`;
    if(tool.program?.modelStep && (this.modelStepCounts.get(countKey)??0)>=tool.modelStepCap) return {failure:{kind:"internal",message:"Skipped — monthly model-step cap reached"} as RunFailure};
    const output = tool.program?.connectionSources?.length?"Demo summary: review the events and bills.\n":"Auckland: fine, high 21C, low 14C.\n";
    for(const source of tool.program?.connectionSources??[]) {
      const connector=this.connector(source.connectorId);
      if(connector.state.kind!=="connected") return {failure:{kind:"internal",message:`Reconnect ${connector.name}. Nothing ran.`} as RunFailure};
      this.outbound.unshift({id:this.id("outbound"),atMs:Date.now(),purpose:"Connection read",destination:connector.name,route:"offline_demo",syntheticOnly:false,costMicros:null,payloadText:JSON.stringify({tool:source.toolName,argsTemplate:JSON.parse(source.argsJson)})});
    }
    if(tool.program?.modelStep) {
      this.modelStepCounts.set(countKey,(this.modelStepCounts.get(countKey)??0)+1);
      this.outbound.unshift({id:this.id("outbound"),atMs:Date.now(),purpose:"Model step",destination:routeLabel(this.status.effectiveRoute),route:this.status.effectiveRoute??"offline_demo",syntheticOnly:true,costMicros:0,payloadText:JSON.stringify({promptBytes:9,dataBytes:2,schemaBytes:130})});
    }
    for (const host of hosts)
      this.outbound.unshift({
        id: this.id("outbound"),
        atMs: Date.now(),
        purpose: "Web source",
        destination: host,
        route: "offline_demo",
        syntheticOnly: false,
        costMicros: null,
        payloadText: `https://${host}/v1/forecast`,
      });
    const run: StagedRun = {
      previewText: output.trimEnd(),
      runId: this.id("run"),
      toolId,
      input: { handleId: "no-input", displayName: "No input file", displayPath: "" },
      outputDir: clone(outputDir),
      outputName,
      planHash: digest({ tool: tool.versionHash, fetched: output, outputDir, outputName }),
      rowsIn: 0,
      rowsOut: 1,
      outputColumns: [],
      previewRows: [],
      destinationExists: this.destinations.has(
        `${outputDir.displayPath}/${outputName}`,
      ),
    };
    this.runRevisions.set(run.runId, this.grantRevisions.get(toolId) ?? 1);
    this.staged.set(run.runId, clone(run));
    this.stagedData.set(run.runId, {
      inputDigest: "no-input",
      outputDigest: digest(output),
    });
    const day=new Date().toLocaleDateString("en-CA");
    const entry: import("./types").TodayEntry={id:this.id("today"),toolId,toolName:tool.name,trigger:"manual",scheduleId:null,startedAtMs:Date.now(),finishedAtMs:null,status:run.plannedCalls?.length?"needs_you":"staged",body:output.slice(0,20000),note:null,fileCreated:null,runId:run.runId,staged:clone(run),notification:false,addToToday:true,dismissed:false};
    const group=this.today.find(d=>d.day===day);if(group)group.entries.unshift(entry);else this.today.unshift({day,entries:[entry]});
    return run;
  }
  async previewRunNoInput(toolId: string) {
    const tool = await this.getTool(toolId);
    const result = await this.stageRunNoInput(
      toolId,
      { handleId: "preview", displayName: "preview", displayPath: "/preview" },
      tool.program?.output_name ?? "answer.txt",
    );
    if (!("failure" in result)) await this.discardRun(result.runId);
    return result;
  }
  async commitRun(runId: string, planHash: string) {
    const run = this.staged.get(runId);
    if (!run)
      return {
        failure: {
          kind: "revoked",
          message: "This preview was canceled. Stage a new preview.",
        } as RunFailure,
      };
    if (run.planHash !== planHash)
      return {
        failure: {
          kind: "input_changed",
          message: "Preview changed. Stage and compare again.",
        } as RunFailure,
      };
    const path = `${run.outputDir.displayPath}/${run.outputName}`;
    if (this.destinations.has(path))
      return {
        failure: {
          kind: "destination_exists",
          message:
            "That output name already exists. Existing files are never replaced.",
        } as RunFailure,
      };
    const tool = this.tools.get(run.toolId);
    if (
      !tool ||
      tool.state === "paused" ||
      this.runRevisions.get(runId) !==
        (this.grantRevisions.get(run.toolId) ?? 1)
    )
      return {
        failure: {
          kind: "revoked",
          message: "Tool was removed.",
        } as RunFailure,
      };
    const stagedData = this.stagedData.get(runId)!;
    const file = this.files.get(run.input.handleId);
    if (file && digest(file.rows) !== stagedData.inputDigest)
      return {
        failure: {
          kind: "input_changed",
          message: "Input changed after staging. Stage and compare again.",
        } as RunFailure,
      };
    const receipt: Receipt = {
      runId,
      toolId: tool.id,
      toolName: tool.name,
      versionHash: tool.versionHash,
      planHash,
      inputDigest: stagedData.inputDigest,
      committedDigest: stagedData.outputDigest,
      outputPath: path,
      status: "committed",
      atMs: Date.now(),
    };
    this.destinations.add(path);
    this.staged.delete(runId);
    this.stagedData.delete(runId);
    this.receipts.unshift(receipt);
    this.today = this.today.map(d=>({...d,entries:d.entries.map(e=>e.runId===runId?{...e,status:"done",staged:null,fileCreated:path,finishedAtMs:Date.now()}:e)}));
    tool.lastRunAtMs = receipt.atMs;
    tool.runCount++;
    this.log("run", `Created ${run.outputName}`, path);
    return clone(receipt);
  }
  async discardRun(runId: string) {
    this.today = this.today.map(d=>({...d,entries:d.entries.map(e=>e.runId===runId?{...e,status:"skipped",body:"Discarded",staged:null,finishedAtMs:Date.now()}:e)}));
    this.staged.delete(runId);
    this.stagedData.delete(runId);
  }
  async revealOutput(receiptRunId: string) {
    if (!this.receipts.some((r) => r.runId === receiptRunId))
      throw new Error("Receipt unavailable.");
  }
  async listActivity() {
    return clone(this.activity);
  }
  async listReceipts() {
    return clone(this.receipts);
  }
  async privacyOverview() {
    await this.listToday();
    return {
      egress: this.status.egress,
      stored: [
        { label: "Today entries", count:this.today.reduce((n,d)=>n+d.entries.length,0), bytes:JSON.stringify(this.today).length, expires:"30 days; items needing you stay until handled" },
        {
          label: "Selected fixtures",
          count: this.files.size,
          bytes: [...this.files.values()].reduce(
            (sum, f) => sum + f.preview.byteSize,
            0,
          ),
          expires: "This browser session",
        },
        {
          label: "Tools",
          count: this.tools.size,
          bytes: JSON.stringify([...this.tools.values()]).length,
          expires: "Until deleted",
        },
        {
          label: "Run receipts",
          count: this.receipts.length,
          bytes: JSON.stringify(this.receipts).length,
          expires: "Until deleted",
        },
        {
          label: "Outbound records",
          count: this.outbound.length,
          bytes: JSON.stringify(this.outbound).length,
          expires: "7 days",
        },
      ],
      outbound: clone(this.outbound),
    };
  }
  async deletePreview() {
    return {
      toolsIncluded: false as const,
      items: [
        { label: "Drafts", count: this.drafts.size },
        { label: "Builds", count: this.builds.size },
        { label: "Build usage legs", count: 0 },
        { label: "Outbound records and payloads", count: this.outbound.length },
        { label: "Connection detections", count: this.suggestions.length },
        { label: "Suggestion dismissals", count: this.dismissed.size },
        { label: "Activity", count: this.activity.length },
        { label: "Today entries", count:this.today.reduce((n,d)=>n+d.entries.length,0) },
        { label: "Schedules", count:this.schedules.size },
        { label: "Staging directories", count: this.staged.size },
        { label: "Sample copies", count: this.sampleInputs.size },
        { label: "Build workspaces", count: 0 },
      ],
    };
  }
  async deleteData(options: { includeTools: boolean }) {
    this.today=[]; this.schedules.clear();
    await this.emergencyStop();
    await this.setLocalOnly(true);
    this.drafts.clear();
    this.builds.clear();
    this.buildDrafts.clear();
    this.files.clear();
    this.sampleInputs.clear();
    this.staged.clear();
    this.stagedData.clear();
    this.runRevisions.clear();
    this.outbound = [];
    this.suggestions = [];
    this.dismissed.clear();
    this.activity = [];
    if (this.show.kind !== "unavailable")
      this.show = { kind: "idle", accessibility: "unknown" };
    this.emitShow();
    if (options.includeTools) {
      this.tools.clear();
      this.grantRevisions.clear();
      this.webHosts.clear();
      this.receipts = [];
    }
  }
  async deleteOutboundHistory() {
    this.outbound = [];
    this.log("settings", "Outbound history deleted");
  }
  async emergencyStop() {
    this.recording.clear();
    for(const s of this.schedules.values()) s.status="emergency_paused";
    for (const b of this.builds.values()) await this.stopBuild(b.buildId);
    this.staged.clear();
    this.stagedData.clear();
    if (this.show.kind === "recording") await this.stopShow();
    this.log("settings", "Emergency stop completed");
  }
  private connector(id: string) {
    const connector = this.connectors.find((c) => c.id === id);
    if (!connector) throw new Error("Connector unavailable.");
    return connector;
  }
  async openExternal(url: string): Promise<void> {
    if (new URL(url).protocol !== "https:")
      throw new Error("Only HTTPS documentation links can be opened.");
    const allowed =
      this.connectors.some((connector) => connector.docsUrl === url) ||
      ["https://dev.meta.ai/", "https://docs.muse.ai/"].includes(url) ||
      [...this.pending].some(
        ([pendingId, connectorId]) =>
          url ===
          `https://example.invalid/authorize?connector=${connectorId}&state=${pendingId}`,
      );
    if (!allowed)
      throw new Error(
        "This link is not in the documentation or authorization catalog.",
      );
    // Browser demo: no external calls.
  }
  async previewConnectorTools(connectorId: string): Promise<ConnectorTool[]> {
    this.connector(connectorId);
    if(connectorId==="google-calendar") return [{name:"list_events",description:"Read calendar events",inputSchema:{type:"object",properties:{start:{type:"string"},end:{type:"string"}},required:["start","end"],additionalProperties:false},readOnly:true,destructive:false}];
    return connectorId === "gmail"
      ? [
          {
            name: "Create draft email",
            description: "Prepare an email draft for review.",
            readOnly: false,
            destructive: false,
          },
          {
            name: "Search threads",
            description: "Find email threads by query.",
            readOnly: true,
            destructive: false,
          },
          {
            name: "Read thread",
            description: "Read messages in an email thread.",
            readOnly: true,
            destructive: false,
          },
        ]
      : [];
  }
  async listConnectors() {
    return clone(this.connectors);
  }
  async setConnectorUrl(connectorId: string, url: string) {
    let parsed: URL;
    try {
      parsed = new URL(url);
    } catch {
      throw new Error("Enter a valid HTTPS MCP URL.");
    }
    if (parsed.protocol !== "https:" || parsed.username || parsed.password)
      throw new Error("Use an HTTPS MCP URL without embedded credentials.");
    const c = this.connector(connectorId);
    if (c.mcpUrl !== url) {
      c.verification = "unverified";
      c.state = { kind: "not_connected" };
      c.hasClientCredentials = false;
      for (const [pendingId, id] of this.pending)
        if (id === connectorId) this.pending.delete(pendingId);
    }
    c.mcpUrl = url;
    return clone(c);
  }
  async setClientCredentials(
    connectorId: string,
    clientId: string,
    _clientSecret: string | null,
  ) {
    if (!clientId.trim()) throw new Error("Enter a client ID first.");
    const c = this.connector(connectorId);
    c.hasClientCredentials = true;
    return clone(c);
  }
  async beginConnect(connectorId: string) {
    const c = this.connector(connectorId);
    if (!c.mcpUrl) throw new Error("Save an HTTPS MCP URL first.");
    if (c.registration === "preregistered" && !c.hasClientCredentials)
      throw new Error(
        `${c.name} requires a client ID before connecting. Save client credentials first.`,
      );
    c.state = { kind: "connecting" };
    const pendingId = this.id("oauth");
    this.pending.set(pendingId, connectorId);
    return {
      connectorId,
      pendingId,
      authorizationUrl: `https://example.invalid/authorize?connector=${connectorId}&state=${pendingId}`,
    };
  }
  async completeConnect(pendingId: string) {
    await delay(1000);
    const id = this.pending.get(pendingId);
    if (!id) throw new Error("Connection canceled.");
    this.pending.delete(pendingId);
    const c = this.connector(id);
    c.state = { kind: "connected", sinceMs: Date.now(), toolCount: 12 };
    this.suggestions = this.suggestions.filter((s) => s.connectorId !== id);
    this.log("connection", `${c.name} connected`, "12 tools available");
    return clone(c);
  }
  async testConnection(connectorId: string) {
    const c = this.connector(connectorId);
    if (c.state.kind !== "connected")
      throw new Error("Connect before testing.");
    this.log("connection", `${c.name}: connection test passed`);
    return clone(c);
  }
  async disconnect(connectorId: string) {
    const c = this.connector(connectorId);
    for (const [pendingId, id] of this.pending)
      if (id === connectorId) this.pending.delete(pendingId);
    c.state = { kind: "not_connected" };
    return clone(c);
  }
  async connectionSuggestions() {
    return clone(
      this.suggestions.filter(
        (s) =>
          !this.dismissed.has(s.connectorId) &&
          this.connector(s.connectorId).state.kind !== "connected",
      ),
    );
  }
  async dismissSuggestion(connectorId: string) {
    this.dismissed.add(connectorId);
    this.suggestions = this.suggestions.filter(
      (s) => s.connectorId !== connectorId,
    );
  }
  private emitShow() {
    this.showListeners.forEach((cb) => cb(clone(this.show)));
  }
  async showState() {
    return clone(this.show);
  }
  async startShow() {
    if (this.show.kind === "recording")
      throw new Error("A recording is already active.");
    const sessionId = this.id("show");
    this.show = {
      kind: "recording",
      sessionId,
      startedAtMs: Date.now(),
      maxSeconds: 300,
      paused: false,
      appsSeen: 0,
      matches: 0,
    };
    this.emitShow();
    setTimeout(() => {
      if (
        this.show.kind === "recording" &&
        this.show.sessionId === sessionId &&
        !this.show.paused
      ) {
        this.show.appsSeen = 1;
        this.show.matches = 1;
        this.emitShow();
      }
    }, 3000);
    setTimeout(() => {
      if (this.show.kind === "recording" && this.show.sessionId === sessionId)
        void this.stopShow();
    }, 300000);
    this.log("show", "Show recording started");
    return clone(this.show);
  }
  async pauseShow(paused: boolean) {
    if (this.show.kind !== "recording")
      throw new Error("Start recording first.");
    this.show.paused = paused;
    this.emitShow();
    return clone(this.show);
  }
  async stopShow() {
    if (this.show.kind !== "recording") return clone(this.show);
    const state = this.show;
    const suggestions: ConnectionSuggestion[] =
      this.connector("gmail").state.kind === "connected" ||
      this.dismissed.has("gmail")
        ? []
        : [
            {
              connectorId: "gmail",
              connectorName: "Gmail",
              message:
                "You used Gmail while recording. Connect Gmail so tools can work with it.",
              signalKind: "browser_host",
              count: 1,
            },
          ];
    this.suggestions = suggestions;
    this.show = {
      kind: "finished",
      sessionId: state.sessionId,
      durationSeconds: Math.min(
        300,
        Math.floor((Date.now() - state.startedAtMs) / 1000),
      ),
      appsSeen: ["Google Chrome", "Numbers"],
      suggestions: clone(suggestions),
    };
    this.emitShow();
    this.log("show", "Show recording finished");
    return clone(this.show);
  }
  async onShowEvent(cb: (state: ShowState) => void) {
    this.showListeners.add(cb);
    return () => {
      this.showListeners.delete(cb);
    };
  }
}

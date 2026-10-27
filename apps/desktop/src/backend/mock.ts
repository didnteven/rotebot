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
      : "Offline demo";

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
  private serial = 0;
  private tools = new Map<string, ToolDetail>();
  private files = new Map<string, { preview: CsvPreview; rows: string[][] }>();
  private drafts = new Map<string, TaskDraft>();
  private builds = new Map<string, BuildState>();
  private buildDrafts = new Map<string, TaskDraft>();
  private buildListeners = new Map<string, Set<(state: BuildState) => void>>();
  private staged = new Map<string, StagedRun>();
  private stagedData = new Map<
    string,
    { inputDigest: string; outputDigest: string }
  >();
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
    this.status.effectiveRoute =
      this.status.egress !== "cloud_allowed"
        ? "offline_demo"
        : this.status.selectedRoute === "offline_demo"
          ? "offline_demo"
          : this.status.apiKey.present
            ? "api_key"
            : this.status.selectedRoute === "muse_plan" &&
                this.status.muse.kind === "signed_in"
              ? "muse_plan"
              : "offline_demo";
    this.status.blockedReason =
      this.status.selectedRoute === "offline_demo"
        ? null
        : this.status.egress !== "cloud_allowed"
          ? "Local only is enabled. Use cloud models in Privacy or choose Offline demo."
          : this.status.effectiveRoute === "offline_demo"
            ? blockedReason
            : null;
    return clone(this.status);
  }
  async appInfo(): Promise<AppInfo> {
    return { version: "0.1.0", platform: "browser" as const, mock: true };
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
  async selectRoute(route: ModelRoute) {
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
      kind, exampleOutput: null,
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
  async updateTask(draftId: string, kind: TaskKind, outputName: string, exampleOutput: string | null) {
    const draft = this.draft(draftId);
    draft.kind = kind; draft.outputName = outputName; draft.exampleOutput = exampleOutput;
    if (kind === "program") draft.questions = [];
    draft.revision++;
    return this.validate(draft);
  }
  async outboundPreview(draftId: string) {
    const draft = this.draft(draftId),
      status = this.resolveRoute(),
      route = status.effectiveRoute ?? "offline_demo";
    return {
      draftId,
      route,
      model: route === "offline_demo" ? "No model" : status.model,
      destination:
        route === "api_key"
          ? "Meta API (api.meta.ai)"
          : route === "muse_plan"
            ? "Muse Code with your Muse plan"
            : "Nothing leaves this Mac",
      payloadText:
        route === "offline_demo"
          ? "Nothing leaves this Mac. The offline demo maps columns without a model."
          : draft.kind === "program" ? JSON.stringify({description: draft.goal, syntheticTextShape: "2123-45-67 HJKLM inventedvalue", exampleRealData: draft.exampleOutput, outputName: draft.outputName}) : JSON.stringify(
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
      containsRealRows: !!draft.exampleOutput,
      estimatedCostMicros:
        route === "muse_plan" ? null : route === "offline_demo" ? 0 : 20000,
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
      routeLabel: routeLabel(preview.route),
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
        purpose: "Build CSV tool",
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
        if (phase === "candidate") build.candidate = clone(draft.mapping);
        if (phase === "ready") {
          const file = this.files.get(draft.input.handle.handleId)!;
          if (draft.kind === "program") {
            // Fixed mock fixture; generated code is never evaluated in the renderer.
            const output = file.rows.map(row => row.join(",")).filter(line => line.includes("ERROR")).join("\n") + "\n";
            build.program = {summary: "Extract ERROR lines", output_name: draft.outputName, code: 'function run() { const text = macroscope.input("input_csv"); macroscope.output(' + JSON.stringify(draft.outputName) + ', text.split("\\n").filter(line => line.includes("ERROR")).join("\\n") + "\\n"); }'};
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
    }
    return clone(build);
  }
  async saveTool(buildId: string, name: string) {
    const build = await this.buildState(buildId);
    if (build.phase !== "ready" || !build.candidate)
      throw new Error("Checks must pass before saving.");
    if (!name.trim()) throw new Error("Give your tool a name.");
    const draft = this.buildDrafts.get(buildId)!;
    if (this.drafts.get(draft.draftId)?.revision !== draft.revision)
      throw new Error("The task changed after this build. Build it again.");
    const tool: ToolDetail = {
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
      access: clone(access),
      versionHash: digest({mapping:build.candidate, program:build.program}),
    };
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
    const file = this.files.get(run.input.handleId)!;
    const stagedData = this.stagedData.get(runId)!;
    if (digest(file.rows) !== stagedData.inputDigest)
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
    tool.lastRunAtMs = receipt.atMs;
    tool.runCount++;
    this.log("run", `Created ${run.outputName}`, path);
    return clone(receipt);
  }
  async discardRun(runId: string) {
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
    return {
      egress: this.status.egress,
      stored: [
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
        { label: "Staging directories", count: this.staged.size },
        { label: "Sample copies", count: this.sampleInputs.size },
        { label: "Build workspaces", count: 0 },
      ],
    };
  }
  async deleteData(options: { includeTools: boolean }) {
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
      this.receipts = [];
    }
  }
  async deleteOutboundHistory() {
    this.outbound = [];
    this.log("settings", "Outbound history deleted");
  }
  async emergencyStop() {
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

import type { ReactNode } from "react";
import { useCallback, useEffect, useRef, useState } from "react";
import type {
  Backend,
  ConnectionSuggestion,
  CsvPreview,
  ModelCostSummary,
  ModelRoute,
  Program,
  Receipt,
  SelectedConnection,
} from "./backend/types";

// Presentation only: opaque handles and original file names still go to the
// backend. Expose invisible controls so a suffix/path cannot impersonate another.
function visibleFileText(text: string) {
  return text.replace(
    /[\u001b\u200b-\u200f\u202a-\u202e\u2060-\u206f\ufeff]/g,
    (c) => `\\u${c.charCodeAt(0).toString(16).padStart(4, "0")}`,
  );
}
export function FileText({ text }: { text: string }) {
  return <bdi dir="ltr">{visibleFileText(text)}</bdi>;
}
export function FileNameInput({ value, onChange, label }: {
  value: string;
  onChange: (value: string) => void;
  label: string;
}) {
  return (
    <>
      <input aria-label={label} dir="ltr" value={value} onChange={(e) => onChange(e.target.value)} />
      {visibleFileText(value) !== value && <FileText text={value} />}
    </>
  );
}
export function isHttpsUrl(value: string) {
  try {
    const url = new URL(value);
    return /^https:\/\//i.test(value) && url.protocol === "https:" &&
      !url.username && !url.password;
  } catch {
    return false;
  }
}

export function useAction() {
  const [busy, setBusy] = useState(false),
    [error, setError] = useState<string | null>(null);
  const pending = useRef(false);
  const lastAction = useRef<() => Promise<unknown>>(async () => {});
  const retry = useCallback(() => lastAction.current(), []);
  async function act(job: () => Promise<unknown>) {
    if (pending.current) return;
    pending.current = true;
    if (job !== retry) lastAction.current = job;
    setBusy(true);
    setError(null);
    try {
      await job();
    } catch (e) {
      setError(
        e instanceof Error ? e.message : "Something went wrong. Try again.",
      );
    } finally {
      pending.current = false;
      setBusy(false);
    }
  }
  return { busy, error, setError, act, retry };
}
export function useScreenFocus<T extends HTMLElement>(screen: unknown) {
  const ref = useRef<T>(null);
  useEffect(() => {
    ref.current?.focus();
    window.scrollTo?.({ top: 0, behavior: "instant" });
    ref.current?.closest("main")?.scrollTo?.({ top: 0, behavior: "instant" });
  }, [screen]);
  return ref;
}
const dollars = (micros: number) => `$${(micros / 1_000_000).toFixed(micros < 10_000 ? 4 : 2)}`;
const callsText = (calls: number) => `${calls} model ${calls === 1 ? "call" : "calls"}`;
/** "$0.0021 (2 model calls)", saying so when the route did not report a price. */
export function costText(micros: number | null, calls: number, unknown: number) {
  const price = unknown >= calls ? "price unknown" : `${dollars(micros ?? 0)}${unknown ? " + unpriced calls" : ""}`;
  return `${price} (${callsText(calls)})`;
}
export function ModelCostLines({ cost }: { cost: ModelCostSummary }) {
  return <ul className="model-cost" aria-label="Model cost">
    {cost.testMicros !== null && <li>Test run: {costText(cost.testMicros, cost.testCalls, cost.testUnknownCalls)}</li>}
    {cost.runCount > 0 && <li>Last run: {costText(cost.lastRunMicros, cost.lastRunCalls, cost.lastRunUnknownCalls)}</li>}
    {cost.runCount > 1 && <li>All {cost.runCount} runs: {dollars(cost.totalMicros)}{cost.unknownCalls ? ` + ${cost.unknownCalls} unpriced` : ""}</li>}
  </ul>;
}
/** 4110073 -> "4.1 MB"; small sizes stay in KB or bytes. */
export function formatBytes(bytes: number) {
  if (bytes >= 1_000_000) return `${(bytes / 1_000_000).toFixed(bytes >= 10_000_000 ? 0 : 1)} MB`;
  if (bytes >= 1_000) return `${Math.round(bytes / 1_000)} KB`;
  return `${bytes} bytes`;
}
export const rowCount = (count: number) => `${count} ${count === 1 ? "row" : "rows"}`;

export function friendlyFailure(message: string, kind?: string): string {
  // A connected service's own refusal is not a website problem, even when its message names a web address.
  if (/^Connection read was rejected by the service/i.test(message)) {
    return /developer preview/i.test(message)
      ? "The service won't let this tool read yet: your Google Cloud project isn't enrolled in the Developer Preview. Try again once it is, or change what the tool should do."
      : "The connected service refused to let the tool read that. Check the details below, then try again or change what the tool should do.";
  }
  const site = message.match(/(?:https?:\/\/)?([a-z0-9](?:[a-z0-9-]*\.)+[a-z]{2,})(?=[/:\s)]|$)/i)?.[1]?.toLowerCase();
  if (site) {
    const reason = /403|401|forbidden|refus|denied/i.test(message) ? " (the site refused the request)"
      : /timeout|timed out/i.test(message) ? " (the site took too long to reply)" : "";
    return `Couldn't read data from ${site}${reason}. Try again, or change what the tool should do.`;
  }
  if (kind === "auth" || kind === "no_model_access") return "Couldn't reach the model. Check your sign-in or key in Settings, then try again.";
  if (kind === "quota" || kind === "budget") return "The model's spending limit has been reached. Check your allowance in Settings, then try again.";
  if (kind === "destination_exists") return "That file already exists. Choose another output name, then try again.";
  if (kind === "input_invalid") return "Couldn't read this file. Check its dates and values, or choose another file, then try again.";
  if (kind === "missing_data") return `The tool can't run yet because it has no data to work with. ${message.trim()} Macroscope won't make data up, so give it a source: choose a file, or check how to get it from the service.`;
  if (kind === "checks") return "The result didn't pass its checks. Try again, or change what the tool should do.";
  return "Couldn't finish this task. Try again, or change what the tool should do.";
}
/** Host of a declared web source URL, with the {today} template filled in. */
export function hostOf(url: string): string {
  try {
    return new URL(url.replace("{today}", "2026-01-01")).hostname || url;
  } catch {
    return url;
  }
}
const knownServices: Record<string, string> = { gmail: "Gmail", "google-calendar": "Google Calendar", "google-drive": "Google Drive" };
export function serviceName(id: string, connections?: { connectorId: string; name: string }[]) {
  return connections?.find(c => c.connectorId === id)?.name ?? knownServices[id] ?? (id.startsWith("api_") ? "your saved API" : id);
}
const aiNames: Record<ModelRoute, string | null> = { api_key: "Meta API key", muse_plan: "Muse plan", custom_endpoint: "Custom endpoint", offline_demo: null };
/** What a tool used, listed after the result instead of asked for before it: services, sites and the AI service. */
export function usedParts(program: Program | null | undefined, ai: ModelRoute | null | undefined, connections?: SelectedConnection[]) {
  const byService = new Map<string, string[]>();
  for (const source of program?.connectionSources ?? []) {
    const service = serviceName(source.connectorId, connections);
    const what = source.name.trim();
    const list = byService.get(service) ?? [];
    if (what && what.toLowerCase() !== service.toLowerCase()) list.push(what.charAt(0).toUpperCase() + what.slice(1));
    byService.set(service, list);
  }
  const services = [...byService].map(([service, what]) => what.length ? `${service} (${[...new Set(what)].join(", ")})` : service);
  const sites = [...new Set((program?.webSources ?? []).map(s => hostOf(s.url)))];
  const aiName = ai ? aiNames[ai] : null;
  return [...services, ...sites, ...(aiName ? [`AI: ${aiName}`] : [])];
}
export function UsedLine({ parts }: { parts: string[] }) {
  if (!parts.length) return null;
  return <p className="used-line"><strong>Used:</strong> {parts.join(" · ")}</p>;
}
export function FailureNotice({ message, kind, retry, change }: { message: string; kind?: string; retry: () => void; change: () => void }) {
  return <>
    <div className="notice notice-warning" role="alert">
      <p>{friendlyFailure(message, kind)}</p>
      <div className="actions"><button onClick={retry}>Try again</button><button onClick={change}>Change the request</button></div>
    </div>
    <details><summary>Details</summary><p>{message}</p></details>
  </>;
}
export function Notice({ error }: { error: string | null }) {
  return error ? (
    <p className="notice notice-warning" role="alert">
      {error}
    </p>
  ) : null;
}
export function Field({
  label,
  children,
}: {
  label: string;
  children: ReactNode;
}) {
  return (
    <label className="field">
      <span>{label}</span>
      {children}
    </label>
  );
}
export function DataTable({
  columns,
  rows,
  caption,
}: {
  columns: string[];
  rows: string[][];
  caption: string;
}) {
  return (
    <div
      className="table-scroll"
      role="region"
      aria-label={caption}
      tabIndex={0}
    >
      <table>
        <caption>{caption}</caption>
        <thead>
          <tr>
            {columns.map((c, i) => (
              <th key={i} scope="col">
                {c}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {rows.map((row, i) => (
            <tr key={i}>
              {row.map((cell, j) => (
                <td key={j}>{cell}</td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
export function CsvView({ preview }: { preview: CsvPreview }) {
  return (
    <section>
      <p>
        <FileText text={preview.handle.displayName} /> · {rowCount(preview.rowCount)} ·{" "}
        {preview.byteSize.toLocaleString()} bytes
      </p>
      {preview.handle.displayPath !== preview.handle.displayName && (
        <p className="muted"><FileText text={preview.handle.displayPath} /></p>
      )}
      {preview.text !== undefined ? <pre aria-label="Input first lines">{preview.text}</pre> : <DataTable
        columns={preview.columns}
        rows={preview.rows}
        caption="Selected CSV · first 8 rows"
      />}
    </section>
  );
}
export function Suggestions({
  suggestions,
  connect,
  dismiss,
}: {
  suggestions: ConnectionSuggestion[];
  connect: (id: string) => void;
  dismiss: (id: string) => void;
}) {
  return (
    <>
      {suggestions.map((s) => (
        <section className="suggestion notice notice-info" key={s.connectorId}>
          <p>{s.message}</p>
          <div className="actions">
            <button onClick={() => connect(s.connectorId)}>
              Connect {s.connectorName}
            </button>
            <button onClick={() => dismiss(s.connectorId)}>Not now</button>
          </div>
        </section>
      ))}
    </>
  );
}
export function ReceiptCard({
  receipt,
  backend,
}: {
  receipt: Receipt;
  backend: Backend;
}) {
  const { act, error, busy } = useAction();
  return (
    <article className="card">
      <h2>Run receipt</h2>
      <p
        className={
          receipt.status === "committed" || receipt.status === "reconciled"
            ? "notice notice-success"
            : "notice notice-warning"
        }
      >
        {receipt.toolName} · {receipt.status === "committed" || receipt.status === "reconciled" ? "Done" : receipt.status === "outcome_unknown" ? "Needs your check" : "Failed"}
      </p>
      {receipt.actions?.map((a, i) => <article key={i}>
        <h3>{a.action.service} · {a.status === "succeeded" ? "Done" : a.status === "failed" ? "Failed" : a.status === "outcome_unknown" ? "Needs your check" : "Not run"}</h3>
        <p>{a.summary}</p>
      </article>)}
      {receipt.outputPath && <p>Created <FileText text={receipt.outputPath} /></p>}
      <details><summary>Details</summary>
      {receipt.actions?.map((a, i) => <div key={i}><p>{a.action.call.toolName}</p><pre>{JSON.stringify(a.action.call.args, null, 2)}</pre></div>)}
      <dl>
        <dt>Version</dt>
        <dd>{receipt.versionHash.slice(0, 12)}</dd>
        <dt>Input digest</dt>
        <dd>{receipt.inputDigest.slice(0, 12)}</dd>
        <dt>Committed digest</dt>
        <dd>{receipt.committedDigest.slice(0, 12)}</dd>
      </dl></details>
      <button
        disabled={busy || !receipt.outputPath}
        onClick={() => void act(() => backend.revealOutput(receipt.runId))}
      >
        Show in Finder
      </button>
      <Notice error={error} />
    </article>
  );
}

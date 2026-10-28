import type { ReactNode } from "react";
import { useCallback, useEffect, useRef, useState } from "react";
import type {
  Backend,
  ConnectionSuggestion,
  CsvPreview,
  Receipt,
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
  const lastAction = useRef<() => Promise<unknown>>(async () => {});
  const retry = useCallback(() => lastAction.current(), []);
  async function act(job: () => Promise<unknown>) {
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
export const rowCount = (count: number) => `${count} ${count === 1 ? "row" : "rows"}`;

export function friendlyFailure(message: string, kind?: string): string {
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
  if (kind === "checks") return "The result didn't pass its checks. Try again, or change what the tool should do.";
  return "Couldn't finish this task. Try again, or change what the tool should do.";
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

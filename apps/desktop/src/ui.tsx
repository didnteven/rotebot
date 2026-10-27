import type { ReactNode } from "react";
import { useState } from "react";
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
  async function act(job: () => Promise<unknown>) {
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
  return { busy, error, setError, act };
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
        <FileText text={preview.handle.displayName} /> · {preview.rowCount} rows ·{" "}
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
        {receipt.toolName} · {receipt.status}
      </p>
      {receipt.actions?.map((a, i) => <article key={i}>
        <h3>{a.action.service} · {a.action.call.toolName} · {a.status}</h3>
        <pre>{JSON.stringify(a.action.call.args, null, 2)}</pre>
        <p>Untrusted server result:</p><pre>{a.summary}</pre>
      </article>)}
      <dl>
        <dt>Version</dt>
        <dd>{receipt.versionHash.slice(0, 12)}</dd>
        <dt>Input digest</dt>
        <dd>{receipt.inputDigest.slice(0, 12)}</dd>
        <dt>Committed digest</dt>
        <dd>{receipt.committedDigest.slice(0, 12)}</dd>
        <dt>Output path</dt>
        <dd><FileText text={receipt.outputPath} /></dd>
      </dl>
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

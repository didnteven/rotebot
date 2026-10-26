import type { ReactNode } from "react";
import { useState } from "react";
import type {
  Backend,
  ConnectionSuggestion,
  CsvPreview,
  Receipt,
} from "./backend/types";

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
    <p className="notice" role="alert">
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
        {preview.handle.displayName} · {preview.rowCount} rows ·{" "}
        {preview.byteSize.toLocaleString()} bytes
      </p>
      <DataTable
        columns={preview.columns}
        rows={preview.rows}
        caption="Selected CSV · first 8 rows"
      />
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
        <section className="suggestion" key={s.connectorId}>
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
      <p>
        {receipt.toolName} · {receipt.status}
      </p>
      <dl>
        <dt>Version</dt>
        <dd>{receipt.versionHash.slice(0, 12)}</dd>
        <dt>Input digest</dt>
        <dd>{receipt.inputDigest.slice(0, 12)}</dd>
        <dt>Committed digest</dt>
        <dd>{receipt.committedDigest.slice(0, 12)}</dd>
        <dt>Output path</dt>
        <dd>{receipt.outputPath}</dd>
      </dl>
      <button
        disabled={busy}
        onClick={() => void act(() => backend.revealOutput(receipt.runId))}
      >
        Show in Finder
      </button>
      <Notice error={error} />
    </article>
  );
}

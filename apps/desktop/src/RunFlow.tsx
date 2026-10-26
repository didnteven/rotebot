import { useState } from "react";
import type {
  Backend,
  CsvPreview,
  FileHandle,
  Receipt,
  RunFailure,
  StagedRun,
  ToolSummary,
} from "./backend/types";
import {
  CsvView,
  DataTable,
  Field,
  Notice,
  ReceiptCard,
  useAction,
} from "./ui";

export function RunFlow({
  backend,
  tool,
  close,
  changed,
}: {
  backend: Backend;
  tool: ToolSummary;
  close: () => void;
  changed: () => Promise<void>;
}) {
  const [input, setInput] = useState<CsvPreview | null>(null),
    [folder, setFolder] = useState<FileHandle | null>(null),
    [name, setName] = useState("orders_ready.csv");
  const [staged, setStaged] = useState<StagedRun | null>(null),
    [receipt, setReceipt] = useState<Receipt | null>(null),
    [failure, setFailure] = useState<RunFailure | null>(null);
  const { act, busy, error } = useAction();
  async function clearStage() {
    if (staged) await backend.discardRun(staged.runId);
    setStaged(null);
    setFailure(null);
  }
  return (
    <section>
      <button
        disabled={busy}
        onClick={() =>
          void act(async () => {
            await clearStage();
            close();
          })
        }
      >
        Back to tools
      </button>
      <h2>Run {tool.name}</h2>
      <p>
        Version {tool.version} · Every new input needs a fresh staged output and
        approval.
      </p>
      <Notice error={error} />
      {busy && <p role="status">Working…</p>}
      {!receipt && (
        <>
          <fieldset disabled={busy || !!staged}>
            <legend>Choose files</legend>
            <div className="actions">
              <button
                onClick={() =>
                  void act(async () => {
                    const next = await backend.pickCsv();
                    if (next) {
                      setInput(next);
                      setFailure(null);
                    }
                  })
                }
              >
                Choose input CSV…
              </button>
              <button
                onClick={() =>
                  void act(async () => {
                    setInput(await backend.loadSampleCsv());
                    setFailure(null);
                  })
                }
              >
                Use sample input
              </button>
              <button
                onClick={() =>
                  void act(async () => {
                    const next = await backend.pickOutputDir();
                    if (next) setFolder(next);
                  })
                }
              >
                Choose output folder…
              </button>
            </div>
            {folder && <p>Output folder: {folder.displayPath}</p>}
            <Field label="Output name">
              <input
                value={name}
                onChange={(e) => {
                  setName(e.target.value);
                  setFailure(null);
                }}
              />
            </Field>
          </fieldset>
          {input && !staged && <CsvView preview={input} />}
          {!staged && (
            <button
              className="primary"
              disabled={busy || !input || !folder || !name.trim()}
              onClick={() =>
                void act(async () => {
                  setFailure(null);
                  const result = await backend.stageRun(
                    tool.id,
                    input!.handle,
                    folder!,
                    name,
                  );
                  if ("failure" in result) setFailure(result.failure);
                  else setStaged(result);
                })
              }
            >
              Stage and compare
            </button>
          )}
          {staged && (
            <section className="card">
              <h3>Staged preview</h3>
              <p>
                {staged.rowsIn} rows → {staged.rowsOut} rows ·{" "}
                {staged.outputColumns.length} columns
              </p>
              <p>
                Plan hash: <code>{staged.planHash.slice(0, 12)}</code>
              </p>
              <p>Source unchanged; existing files are never replaced</p>
              <DataTable
                columns={staged.outputColumns}
                rows={staged.previewRows}
                caption="Previewed CSV · first 8 rows"
              />
              {staged.destinationExists && (
                <p className="notice notice-warning">
                  This destination already exists. Choose another name before
                  creating the CSV.
                </p>
              )}
              <div className="actions">
                <button disabled={busy} onClick={() => void act(clearStage)}>
                  Cancel
                </button>
                <button
                  className="primary"
                  disabled={busy}
                  onClick={() =>
                    void act(async () => {
                      const result = await backend.commitRun(
                        staged.runId,
                        staged.planHash,
                      );
                      if ("failure" in result) setFailure(result.failure);
                      else {
                        setReceipt(result);
                        setStaged(null);
                        await changed();
                      }
                    })
                  }
                >
                  Create previewed CSV
                </button>
              </div>
            </section>
          )}
          {failure && (
            <div className="notice notice-warning" role="alert">
              <p>{failure.message}</p>
              {failure.kind === "input_invalid" && (
                <p>
                  Row: {failure.row ?? "unknown"} · Column:{" "}
                  {failure.column ?? "unknown"}
                </p>
              )}
              <button disabled={busy} onClick={() => void act(clearStage)}>
                {failure.kind === "destination_exists"
                  ? "Choose another name"
                  : failure.kind === "input_invalid"
                    ? "Choose another input"
                    : "Stage again"}
              </button>
            </div>
          )}
        </>
      )}
      {receipt && (
        <>
          <ReceiptCard receipt={receipt} backend={backend} />
          <button
            onClick={() => {
              setReceipt(null);
              setFailure(null);
            }}
          >
            Run again
          </button>
        </>
      )}
    </section>
  );
}

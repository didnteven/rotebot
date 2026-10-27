import { useEffect, useState } from "react";
import type {
  Backend,
  CsvPreview,
  FileHandle,
  Receipt,
  RunFailure,
  StagedRun,
  ToolSummary,
  ToolDetail,
} from "./backend/types";
import {
  CsvView,
  DataTable,
  Field,
  FileNameInput,
  FileText,
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
  const [detail, setDetail] = useState<ToolDetail | null>(null);
  useEffect(() => { void backend.getTool(tool.id).then(next => { setDetail(next); if (next.program) setName(next.program.output_name); }); }, [backend, tool.id]);
  const [input, setInput] = useState<CsvPreview | null>(null),
    [folder, setFolder] = useState<FileHandle | null>(null),
    [name, setName] = useState("orders_ready.csv");
  const [staged, setStaged] = useState<StagedRun | null>(null),
    [receipt, setReceipt] = useState<Receipt | null>(null),
    [failure, setFailure] = useState<RunFailure | null>(null);
  const [actionsReviewed, setActionsReviewed] = useState(false);
  const { act, busy, error } = useAction();
  async function clearStage() {
    if (staged) await backend.discardRun(staged.runId);
    setStaged(null);
    setActionsReviewed(false);
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
      {detail?.program && <><p>Program (sandboxed)</p><details><summary>Advanced details</summary><pre>{detail?.program.code}</pre></details></>}
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
                Choose input file…
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
            {folder && <p>Output folder: <FileText text={folder.displayPath} /></p>}
            <Field label="Output name">
              <FileNameInput
                label="Output name"
                value={name}
                onChange={(value) => {
                  setName(value);
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
                  else { setStaged(result); setActionsReviewed(false); }
                })
              }
            >
              Stage and compare
            </button>
          )}
          {staged && (
            <section className="card">
              <h3>Staged preview</h3>
              {staged.createsOutput === false ? <p>Connection actions only</p> : <p>
                {staged.rowsIn} rows → {staged.rowsOut} rows ·{" "}
                {staged.outputColumns.length} columns
              </p>}
              <p>
                Plan hash: <code>{staged.planHash.slice(0, 12)}</code>
              </p>
              <p>Source unchanged; existing files are never replaced</p>
              {staged.createsOutput === false ? null : staged.previewText !== undefined ? <pre aria-label="Staged output first lines">{staged.previewText}</pre> : <DataTable
                columns={staged.outputColumns}
                rows={staged.previewRows}
                caption="Previewed CSV · first 8 rows"
              />}
              {(staged.plannedCalls ?? []).map((a, i) => <article key={i} aria-label={`Planned action ${i + 1}`}>
                <h4>{a.service} · {a.call.toolName}</h4>
                <pre>{JSON.stringify(a.call.args, null, 2)}</pre>
                <p>Server’s claims: readOnlyHint = {String(a.readOnlyHint)}; destructiveHint = {String(a.destructiveHint)}</p>
              </article>)}
              {staged.plannedCalls?.some(a => a.readOnlyHint !== true || a.destructiveHint === true) && <label>
                <input type="checkbox" checked={actionsReviewed} onChange={e => setActionsReviewed(e.target.checked)} />I reviewed these actions
              </label>}
              {staged.destinationExists && (
                <p className="notice notice-warning">
                  This destination already exists. Choose another name before
                  creating the output.
                </p>
              )}
              <div className="actions">
                <button disabled={busy} onClick={() => void act(clearStage)}>
                  Cancel
                </button>
                <button
                  className="primary"
                  disabled={busy || (!!staged.plannedCalls?.some(a => a.readOnlyHint !== true || a.destructiveHint === true) && !actionsReviewed)}
                  onClick={() =>
                    void act(async () => {
                      const result = await backend.commitRun(
                        staged.runId,
                        staged.planHash,
                        actionsReviewed,
                      );
                      if ("failure" in result) setFailure(result.failure);
                      else {
                        setReceipt(result);
                        setStaged(null);
                        setActionsReviewed(false);
                        await changed();
                      }
                    })
                  }
                >
                  {staged.plannedCalls?.length ? (staged.createsOutput === false ? "Approve actions" : "Approve actions and create output") : tool.engine === "js-quickjs" ? "Create previewed output" : "Create previewed CSV"}
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

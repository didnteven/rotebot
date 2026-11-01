import { BotAvatar, BotStatus, botIdentity } from "./Bot";
import { ActionReview, ActionTechnicalDetails } from "./ActionReview";
import { SourceReview, SourceTechnicalDetails } from "./SourceReview";
import { ScheduleSection } from "./Schedule";
import { useEffect, useRef, useState } from "react";
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
  FailureNotice,
  ModelCostLines,
  rowCount,
  useScreenFocus,
  ReceiptCard,
  useAction,
} from "./ui";

export function RunFlow({
  backend,
  tool,
  close,
  changed,
  openSettings,
  scheduleFirst = false,
}: {
  backend: Backend;
  tool: ToolSummary;
  close: () => void;
  changed: () => Promise<void>;
  openSettings?: () => void;
  /** Opened from the Schedule button: show the schedule section already expanded. */
  scheduleFirst?: boolean;
}) {
  const { act, busy, error, retry, setError } = useAction();
  const [scheduleShown, setScheduleShown] = useState(scheduleFirst);
  const [modelRoute,setModelRoute]=useState<import("./backend/types").ModelRoute>("offline_demo");
  const [routeDetails,setRouteDetails]=useState("");
  const [detail, setDetail] = useState<ToolDetail | null>(null);
  const [loadingDetail, setLoadingDetail] = useState(true);
  const detailRequest = useRef(0);
  async function loadDetail() {
    const request = ++detailRequest.current;
    setLoadingDetail(true);
    setError(null);
    try {
      const [access, next] = await Promise.all([backend.modelAccess(), backend.getTool(tool.id)]);
      if (request !== detailRequest.current) return;
      setModelRoute(access.effectiveRoute ?? "offline_demo");
      setRouteDetails(`Custom endpoint · ${access.customEndpoint.host} · ${access.customEndpoint.model}`);
      setDetail(next);
      if (next.program) setName(next.program.output_name);
    } catch (error) {
      if (request === detailRequest.current) setError(error instanceof Error ? error.message : "Could not load your tool.");
    } finally {
      if (request === detailRequest.current) setLoadingDetail(false);
    }
  }
  useEffect(() => {
    setDetail(null);
    void loadDetail();
    return () => { detailRequest.current++; };
  }, [backend, tool.id]);
  const [input, setInput] = useState<CsvPreview | null>(null),
    [folder, setFolder] = useState<FileHandle | null>(null),
    [name, setName] = useState("orders_ready.csv");
  const [preview, setPreview] = useState<StagedRun | null>(null);
  const [staged, setStaged] = useState<StagedRun | null>(null),
    [receipt, setReceipt] = useState<Receipt | null>(null),
    [failure, setFailure] = useState<RunFailure | null>(null);
  const [actionsReviewed, setActionsReviewed] = useState(false);
  const headingRef = useScreenFocus<HTMLHeadingElement>(receipt ? "receipt" : staged ? "preview" : "input");
  async function clearStage() {
    if (staged) await backend.discardRun(staged.runId);
    setStaged(null);
    setActionsReviewed(false);
    setFailure(null);
  }
  async function stage() {
    setFailure(null);
    const result = detail?.noInput
      ? await backend.stageRunNoInput(tool.id, folder!, name)
      : await backend.stageRun(tool.id, input!.handle, folder!, name);
    if ("failure" in result) setFailure(result.failure);
    else { setStaged(result); setActionsReviewed(false); }
  }
  const previewMode = !!detail?.noInput && !!detail.program;
  // A tool that plans an email or other change needs no output file: the actions are the point.
  const sendsOnly = previewMode && !!preview?.plannedCalls?.some(a => a.readOnlyHint !== true || a.destructiveHint === true);
  async function reviewActions() {
    setFailure(null);
    const result = await backend.stageActionsNoInput(tool.id);
    if ("failure" in result) setFailure(result.failure);
    else { setStaged(result); setActionsReviewed(false); }
  }
  async function previewFirst() {
    setFailure(null);
    const result = await backend.previewRunNoInput(tool.id);
    if ("failure" in result) setFailure(result.failure);
    else setPreview(result);
  }
  // The person has already read the answer, so when the real run matches it
  // and plans nothing else, creating the file is the one click they just made.
  async function createFromPreview() {
    setFailure(null);
    const result = await backend.stageRunNoInput(tool.id, folder!, name);
    if ("failure" in result) { setFailure(result.failure); return; }
    const same = result.previewText === preview?.previewText && !result.plannedCalls?.length && !result.destinationExists;
    if (!same) { setStaged(result); setActionsReviewed(false); return; }
    const receipt = await backend.commitRun(result.runId, result.planHash, false);
    if ("failure" in receipt) { setStaged(result); setFailure(receipt.failure); return; }
    setReceipt(receipt);
    await changed();
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
      <BotAvatar seed={botIdentity(tool.id)} state={busy ? "working" : failure ? "error" : receipt ? "success" : staged ? "waiting" : "idle"} size={96} />
      <h2 ref={headingRef} tabIndex={-1}>{receipt ? "Run finished" : staged ? "Result preview" : `Run ${tool.name}`}</h2>
      {detail?.program && <SourceReview advanced={false} program={detail.program} route={modelRoute} routeDetails={routeDetails}/>}
      {scheduleFirst && detail && !detail.noInput && <p role="note" className="notice notice-info">This tool needs a file each time, so it can’t run on a schedule. Only tools that fetch their own data can.</p>}
      {detail?.noInput && <details open={scheduleShown} onToggle={e => setScheduleShown(e.currentTarget.open)}><summary>Schedule this tool</summary><ScheduleSection advanced={false} backend={backend} tool={detail} openSettings={openSettings} onChanged={() => void changed()} /></details>}
      <p>Check the result before creating the output file.</p>
      {(busy || loadingDetail) && <BotStatus seed={botIdentity(tool.id)} label={!detail ? "Loading your tool…" : staged ? "Updating your run…" : "Preparing your result…"} />}
      {detail?.noInput && detail?.program && detail.program.webSources.length > 0 && (
        <p role="note" className="notice notice-info">
          This run reads data from{" "}
          {[...new Set(detail.program.webSources.map(s => { try { return new URL(s.url.replace("{today}", "2026-01-01")).hostname; } catch { return s.url; } }))].join(", ")}
          . These are the sites you approved for this tool. Reads are recorded under Privacy.
        </p>
      )}
      {detail?.noInput && (
        <p>No input file: this tool reads the sites you approved.</p>
      )}
      {error && <FailureNotice message={error} retry={() => void act(detail ? retry : loadDetail)} change={() => void act(clearStage)} />}

      {!receipt && (
        <>
          {detail?.noInput && !!detail.program && !preview && !staged && (
            <button className="primary" disabled={busy || !detail} onClick={() => void act(previewFirst)}>Preview result</button>
          )}
          {detail?.noInput && !!preview && !staged && (
            <section className="card">
              <h3>Result</h3>
              <div className="result-answer preserve-lines" aria-label="Answer">{preview.previewText}</div>
              {sendsOnly && <p>This tool sends or changes something, so it needs your approval. It does not need an output file.</p>}
              <div className="actions">
                {sendsOnly && <button className="primary" disabled={busy} onClick={() => void act(reviewActions)}>Review actions</button>}
                <button disabled={busy} onClick={() => void act(previewFirst)}>Check again</button>
              </div>
            </section>
          )}
          {(!detail?.noInput || (!!preview && !sendsOnly) || !!staged || !detail.program) && <fieldset disabled={busy || !!staged}>
            <legend>{detail?.noInput ? "Save the result" : "Choose files"}</legend>
            <p>Choose where to save the new file. Your original file stays unchanged.</p>
            <div className="actions">
              {!detail?.noInput && (
                <>
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
                    Choose input file
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
                </>
              )}
              <button
                onClick={() =>
                  void act(async () => {
                    const next = await backend.pickOutputDir();
                    if (next) setFolder(next);
                  })
                }
              >
                Choose output folder
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
          </fieldset>}
          {input && !staged && !detail?.noInput && <CsvView preview={input} />}
          {!staged && !sendsOnly && (!previewMode || !!preview) && (
            <>
            {(busy || !detail || (!detail?.noInput && !input) || !folder || !name.trim()) && <p id="run-reason">{busy ? "Working…" : !detail ? "Loading your tool…" : !detail?.noInput && !input ? "Choose an input file and output folder to continue." : !folder ? "Choose an output folder to continue." : "Give the output file a name."}</p>}
            <button
              className="primary"
              aria-describedby="run-reason"
              disabled={busy || !detail || (!detail?.noInput && !input) || !folder || !name.trim()}
              onClick={() => void act(previewMode ? createFromPreview : stage)}
            >
              {previewMode ? "Create file" : "Preview result"}
            </button>
            </>
          )}
          {staged && (
            <section className="card">

              {staged.createsOutput === false ? <p>Connection actions only</p> : !detail?.noInput && !detail?.program?.webSources.length && <p>{rowCount(staged.rowsIn)} → {rowCount(staged.rowsOut)} · {staged.outputColumns.length} {staged.outputColumns.length === 1 ? "column" : "columns"}</p>}
              <p>Your input stays unchanged; existing files are never replaced</p>
              {staged.createsOutput === false ? null : staged.previewText !== undefined ? <div className="result-answer preserve-lines" aria-label="Answer">{staged.previewText}</div> : <DataTable
                columns={staged.outputColumns}
                rows={staged.previewRows}
                caption="Previewed CSV · first 8 rows"
              />}
              {(staged.plannedCalls ?? []).map((a, i) => <article key={i} aria-label={`Planned action ${i + 1}`}>
                <ActionReview action={a}/>
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
              {!!staged.plannedCalls?.some(a => a.readOnlyHint !== true || a.destructiveHint === true) && !actionsReviewed && <p>Review and approve the actions above to continue.</p>}

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
          {failure && <FailureNotice message={failure.message} kind={failure.kind} retry={() => void act(async () => {
            if (staged) {
              const result = await backend.commitRun(staged.runId, staged.planHash, actionsReviewed);
              if ("failure" in result) setFailure(result.failure);
              else { setReceipt(result); setStaged(null); setFailure(null); await changed(); }
            } else await stage();
          })} change={() => void act(clearStage)} />}
          {failure?.kind === "input_invalid" && <details><summary>File details</summary><p>Row: {failure.row ?? "unknown"} · Column: {failure.column ?? "unknown"}</p></details>}
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
      {detail?.modelCost && <section aria-label="Model cost"><h3>Model cost</h3><ModelCostLines cost={detail.modelCost}/><p className="muted">Billed to your model route. Each run's cost is added after it finishes.</p></section>}
      <details><summary>Details</summary>
        <p>{detail?.program ? "Program (sandboxed) · " : ""}Version {tool.version}</p>
        {detail?.program?.modelStep && <label>Monthly model-step cap<input aria-label="Monthly model-step cap" type="number" min={0} max={10000} value={detail.modelStepCap} onChange={e=>{const cap=Number(e.target.value);void act(async()=>setDetail(await backend.setModelStepCap(tool.id,cap)));}}/></label>}
        {detail?.program && <><pre>{detail.program.code}</pre><SourceTechnicalDetails program={detail.program}/></>}
        {staged && <><p>Plan hash: <code>{staged.planHash}</code></p><ActionTechnicalDetails actions={staged.plannedCalls??[]}/></>}
      </details>
    </section>
  );
}

import { useEffect, useState } from "react";
import type {
  Backend,
  BuildState,
  ConnectorInfo,
  CsvMapping,
  CsvPreview,
  DateFormat,
  ModelAccessStatus,
  OutboundPreview,
  TaskDraft,
  Transform,
} from "./backend/types";
import { CsvView, DataTable, Field, FileNameInput, Notice, useAction } from "./ui";

const formats: DateFormat[] = [
  "mm_dd_yy",
  "mm_dd_yyyy",
  "dd_mm_yy",
  "dd_mm_yyyy",
  "yyyy_mm_dd",
  "iso8601",
];
const transforms: Transform["type"][] = [
  "copy",
  "trim",
  "upper",
  "lower",
  "date",
  "money",
  "constant",
];
const formatLabels: Record<DateFormat, string> = {
  mm_dd_yy: "MM/DD/YY",
  mm_dd_yyyy: "MM/DD/YYYY",
  dd_mm_yy: "DD/MM/YY",
  dd_mm_yyyy: "DD/MM/YYYY",
  yyyy_mm_dd: "YYYY-MM-DD",
  iso8601: "ISO 8601 (YYYY-MM-DD)",
};
const transformLabels: Record<Transform["type"], string> = {
  copy: "Copy as is",
  trim: "Trim spaces",
  upper: "UPPERCASE",
  lower: "lowercase",
  date: "Date",
  money: "Money",
  constant: "Fixed value",
};
function newTransform(type: Transform["type"]): Transform {
  return type === "date"
    ? { type, from: "mm_dd_yy", to: "iso8601" }
    : type === "money"
      ? { type, currency: "USD", decimals: 2 }
      : type === "constant"
        ? { type, value: "" }
        : { type };
}
const terminal = (build: BuildState) =>
  ["ready", "failed", "stopped"].includes(build.phase);

/** Host of a declared web source URL, with the {today} template filled in. */
function hostOf(url: string): string {
  try {
    return new URL(url.replace("{today}", "2026-01-01")).hostname || url;
  } catch {
    return url;
  }
}

export function CreateFlow({
  backend,
  initialInput,
  status,
  connectors,
  openSettings,
  connect,
  close,
  saved,
}: {
  backend: Backend;
  initialInput: CsvPreview | null;
  status: ModelAccessStatus;
  connectors: ConnectorInfo[];
  openSettings: () => void;
  connect: (id: string) => void;
  close: () => void;
  saved: () => Promise<void>;
}) {
  const [step, setStep] = useState(0),
    [input, setInput] = useState(initialInput),
    [noInput, setNoInput] = useState(false),
    [description, setDescription] = useState(
      initialInput && initialInput.text === undefined
        ? "Clean this orders CSV for import: Order, Date, Amount."
        : "",
    );
  const [draft, setDraft] = useState<TaskDraft | null>(null),
    [map, setMap] = useState<CsvMapping | null>(null),
    [outputName, setOutputName] = useState("orders_ready.csv");
  const [preview, setPreview] = useState<OutboundPreview | null>(null),
    [reviewed, setReviewed] = useState(false),
    [build, setBuild] = useState<BuildState | null>(null),
    [hostsApproved, setHostsApproved] = useState(false),
    [name, setName] = useState("Clean import CSV");
  const [example, setExample] = useState("");
  const { act, busy, error } = useAction();
  const buildId = build?.buildId;
  useEffect(() => {
    if (!buildId) return;
    let active = true;
    let unlisten: (() => void) | undefined;
    void backend
      .onBuildEvent(buildId, (state) => {
        if (active) setBuild(state);
      })
      .then((fn) => {
        if (!active) fn();
        else unlisten = fn;
      })
      .catch(() => {
        /* The state request and polling below provide recovery. */
      });
    const poll = setInterval(() => {
      void backend
        .buildState(buildId)
        .then((state) => {
          if (active) setBuild(state);
          if (terminal(state)) clearInterval(poll);
        })
        .catch(() => {});
    }, 1000);
    void backend
      .buildState(buildId)
      .then((state) => {
        if (active) setBuild(state);
      })
      .catch(() => {});
    return () => {
      active = false;
      unlisten?.();
      clearInterval(poll);
    };
  }, [backend, buildId]);
  useEffect(() => {
    setReviewed(false);
    setPreview(null);
  }, [status.effectiveRoute, status.selectedRoute, status.egress]);
  useEffect(() => {
    if (step === 2 && !preview)
      void act(async () => {
        setPreview(await backend.outboundPreview(draft!.draftId));
      });
  }, [step, preview, backend, draft]); // review is invalidated when the route changes
  function acceptDraft(next: TaskDraft) {
    setDraft(next);
    setMap(next.mapping);
    setOutputName(next.outputName);
    setPreview(null);
    setReviewed(false);
    setHostsApproved(false);
  }
  function editColumn(
    index: number,
    update: Partial<CsvMapping["output_columns"][number]>,
  ) {
    if (!map) return;
    setMap({
      ...map,
      output_columns: map.output_columns.map((c, i) =>
        i === index ? { ...c, ...update } : c,
      ),
    });
  }
  async function review() {
    const next = draft!.kind === "program"
      ? await backend.updateTask(draft!.draftId, "program", outputName, example || null)
      : await backend.updateMapping(draft!.draftId, map!, outputName);
    acceptDraft(next);
    if (next.readyToBuild) {
      setPreview(await backend.outboundPreview(next.draftId));
      setStep(2);
    }
  }
  const activeBuild = build && !terminal(build);
  return (
    <section aria-label="Create tool">
      <div className="actions">
        <button
          disabled={busy}
          onClick={() => {
            if (step === 0) close();
            else setStep(step - 1);
          }}
        >
          Back
        </button>
        <span className="muted">Create tool</span>
      </div>
      <ol className="stepper" aria-label="Create progress">
        {[
          "Describe",
          "Task summary",
          "Review outbound",
          "Build and compare",
        ].map((label, i) => (
          <li key={label} aria-current={i === step ? "step" : undefined}>
            {i + 1}. {label}
          </li>
        ))}
      </ol>
      <Notice error={error} />
      {activeBuild && step !== 3 && (
        <div role="note" className="notice notice-info">
          <p>A build is active: {build.phase}</p>
          <button onClick={() => setStep(3)}>Return to build</button>
          <button
            disabled={busy}
            onClick={() =>
              void act(async () =>
                setBuild(await backend.stopBuild(build.buildId)),
              )
            }
          >
            Stop build
          </button>
        </div>
      )}
      {busy && <p role="status">Working…</p>}
      {step === 0 && (
        <>
          <h2>Describe a task</h2>
          <Field label="What do you want to do with this file?">
            <textarea
              rows={4}
              value={description}
              onChange={(e) => setDescription(e.target.value)}
            />
          </Field>
          <div className="actions">
            <button
              disabled={busy}
              onClick={() =>
                void act(async () => {
                  const next = await backend.pickCsv();
                  if (next) {
                    setInput(next);
                    setNoInput(false);
                  }
                })
              }
            >
              Choose text file…
            </button>
            <button
              disabled={busy}
              onClick={() =>
                void act(async () => {
                  setInput(await backend.loadSampleCsv());
                  setNoInput(false);
                })
              }
            >
              Use the sample CSV
            </button>
            <button
              disabled={busy}
              onClick={() => {
                setInput(null);
                setNoInput(true);
              }}
            >
              No input file
            </button>
          </div>
          {input && !noInput && <CsvView preview={input} />}
          {noInput && (
            <p role="note" className="notice notice-info">
              No input file — the program will read approved web sources
              (for example a weather API) instead of a file you choose.
            </p>
          )}
          <button
            className="primary"
            disabled={busy || (!input && !noInput) || !description.trim()}
            onClick={() =>
              void act(async () => {
                acceptDraft(
                  noInput
                    ? await backend.draftTaskNoInput(description)
                    : await backend.draftTask(input!.handle, description),
                );
                setStep(1);
              })
            }
          >
            Review task summary
          </button>
        </>
      )}
      {step === 1 && draft && map && (
        <>
          <h2>Task summary</h2>
          {draft.noInput && (
            <p role="note" className="notice notice-info">
              No input file: only a sandboxed program reading approved web
              sources can do this task.
            </p>
          )}
          <Field label="Task kind">
            <select value={draft.kind ?? "csv-mapping"} disabled={busy || draft.noInput} onChange={(e) => void act(async () => {
              const kind = e.target.value as "csv-mapping" | "program";
              acceptDraft(await backend.updateTask(draft.draftId, kind, outputName.replace(/\.[^.]+$/, kind === "program" ? ".txt" : ".csv"), example || null));
            })}>
              <option value="csv-mapping">CSV mapping</option>
              <option value="program">Program (sandboxed)</option>
            </select>
          </Field>
          {draft.kind === "program" && <Field label="Example of the expected result (optional)">
            <textarea aria-label="Example of the expected result (optional)" rows={4} maxLength={16000} value={example} onChange={(e) => setExample(e.target.value)} />
            <span>This is your real text. It will be included in the outbound review.</span>
          </Field>}
          <ul>
            {draft.summary.map((line) => (
              <li key={line}>{line}</li>
            ))}
          </ul>
          {draft.questions.map((question) => (
            <fieldset key={question.id} disabled={busy}>
              <legend>{question.prompt}</legend>
              {question.options.map((o) => (
                <label className="choice" key={o.value}>
                  <input
                    type="radio"
                    name={question.id}
                    value={o.value}
                    checked={question.answer === o.value}
                    onChange={() =>
                      void act(async () => {
                        await backend.updateMapping(
                          draft.draftId,
                          map,
                          outputName,
                        );
                        acceptDraft(
                          await backend.answerQuestion(
                            draft.draftId,
                            question.id,
                            o.value,
                          ),
                        );
                      })
                    }
                  />
                  {o.label}
                </label>
              ))}
            </fieldset>
          ))}
          {draft.kind !== "program" && <div
            className="table-scroll"
            role="region"
            aria-label="Column mapping"
            tabIndex={0}
          >
            <table>
              <caption>Column mapping · edit in this list</caption>
              <thead>
                <tr>
                  <th>Output column</th>
                  <th>From column</th>
                  <th>Transform</th>
                  <th>Format or currency</th>
                </tr>
              </thead>
              <tbody>
                {map.output_columns.map((column, i) => (
                  <tr key={i}>
                    <td>
                      <input
                        aria-label={`Output column ${i + 1}`}
                        value={column.name}
                        onChange={(e) =>
                          editColumn(i, { name: e.target.value })
                        }
                      />
                    </td>
                    <td>
                      <select
                        aria-label={`From column ${i + 1}`}
                        value={column.source}
                        onChange={(e) =>
                          editColumn(i, { source: e.target.value })
                        }
                      >
                        <option value="">Choose column</option>
                        {draft.input.columns.map((c) => (
                          <option key={c}>{c}</option>
                        ))}
                      </select>
                    </td>
                    <td>
                      <select
                        aria-label={`Transform ${i + 1}`}
                        value={column.transform.type}
                        onChange={(e) =>
                          editColumn(i, {
                            transform: newTransform(
                              e.target.value as Transform["type"],
                            ),
                          })
                        }
                      >
                        {transforms.map((t) => (
                          <option key={t} value={t}>
                            {transformLabels[t]}
                          </option>
                        ))}
                      </select>
                    </td>
                    <td>
                      {column.transform.type === "date" && (
                        <>
                          <Field label="From format">
                            <select
                              aria-label={`Date input format ${i + 1}`}
                              value={column.transform.from}
                              onChange={(e) =>
                                editColumn(i, {
                                  transform: {
                                    ...(column.transform as Extract<
                                      Transform,
                                      { type: "date" }
                                    >),
                                    from: e.target.value as DateFormat,
                                  },
                                })
                              }
                            >
                              {formats.map((f) => (
                                <option key={f} value={f}>
                                  {formatLabels[f]}
                                </option>
                              ))}
                            </select>
                          </Field>
                          <Field label="To format">
                            <select
                              aria-label={`Date output format ${i + 1}`}
                              value={column.transform.to}
                              onChange={(e) =>
                                editColumn(i, {
                                  transform: {
                                    ...(column.transform as Extract<
                                      Transform,
                                      { type: "date" }
                                    >),
                                    to: e.target.value as DateFormat,
                                  },
                                })
                              }
                            >
                              {formats.map((f) => (
                                <option key={f} value={f}>
                                  {formatLabels[f]}
                                </option>
                              ))}
                            </select>
                          </Field>
                        </>
                      )}
                      {column.transform.type === "money" && (
                        <>
                          <Field label="Currency">
                            <input
                              aria-label={`Currency ${i + 1}`}
                              value={column.transform.currency}
                              onChange={(e) =>
                                editColumn(i, {
                                  transform: {
                                    ...(column.transform as Extract<
                                      Transform,
                                      { type: "money" }
                                    >),
                                    currency: e.target.value,
                                  },
                                })
                              }
                            />
                          </Field>
                          <Field label="Decimal places">
                            <input
                              aria-label={`Decimal places ${i + 1}`}
                              type="number"
                              min={0}
                              max={6}
                              value={column.transform.decimals}
                              onChange={(e) =>
                                editColumn(i, {
                                  transform: {
                                    ...(column.transform as Extract<
                                      Transform,
                                      { type: "money" }
                                    >),
                                    decimals: Number(e.target.value),
                                  },
                                })
                              }
                            />
                          </Field>
                        </>
                      )}
                      {column.transform.type === "constant" && (
                        <Field label="Value">
                          <input
                            aria-label={`Constant value ${i + 1}`}
                            value={column.transform.value}
                            onChange={(e) =>
                              editColumn(i, {
                                transform: {
                                  type: "constant",
                                  value: e.target.value,
                                },
                              })
                            }
                          />
                        </Field>
                      )}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>}
          <Field label="Output file name">
            <FileNameInput
              label="Output file name"
              value={outputName}
              onChange={setOutputName}
            />
          </Field>
          {draft.blockers.length > 0 && (
            <div className="notice notice-warning">
              <p>Before you build</p>
              <ul>
                {draft.blockers.map((b) => (
                  <li key={b}>{b}</li>
                ))}
              </ul>
            </div>
          )}
          <fieldset disabled={busy || !!activeBuild}>
            <legend>Connections this tool may use</legend>
            {connectors.filter(c => c.state.kind === "connected").map(c => <div key={c.id}>
              <label><input type="checkbox" checked={draft.requiredConnections?.some(s => s.connectorId === c.id) ?? false}
                onChange={event => { const checked = event.target.checked; void act(async () => {
                  const ids = (draft.requiredConnections ?? []).map(s => s.connectorId).filter(id => id !== c.id);
                  if (checked) ids.push(c.id);
                  acceptDraft(await backend.selectTaskConnections(draft.draftId, ids));
                }); }} />Let this tool use {c.name}</label>
              {draft.requiredConnections?.filter(s => s.connectorId === c.id).map(s => <ul key={s.connectorId}>{s.tools.map(t => <li key={t.name}><code>{t.name}</code>: {t.description}</li>)}</ul>)}
            </div>)}
          </fieldset>
          {draft.mentionedConnectors
            .map((id) => connectors.find((c) => c.id === id))
            .filter((c) => c && c.state.kind !== "connected")
            .map(
              (c) =>
                c && (
                  <div
                    role="note"
                    className="suggestion notice notice-info"
                    key={c.id}
                  >
                    <p>
                      You mentioned {c.name}. Connect it in Connections so this
                      tool can use it.
                    </p>
                    <button onClick={() => connect(c.id)}>
                      Connect {c.name}
                    </button>
                  </div>
                ),
            )}
          <button
            className="primary"
            disabled={
              busy || draft.questions.some((q) => !q.answer) || !!activeBuild
            }
            onClick={() => void act(review)}
          >
            Review what leaves this Mac
          </button>
        </>
      )}
      {step === 2 && (
        <>
          <h2>Review what leaves this Mac</h2>
          {status.effectiveRoute === null && (
            <div className="notice notice-warning">
              <p>{status.blockedReason ?? "No build route is available."}</p>
              <button onClick={openSettings}>Open model settings</button>
            </div>
          )}
          {preview && (
            <>
              {preview.route === "offline_demo" ? (
                <>
                  <p className="notice notice-info">
                    {draft?.kind === "program" ? "Writing a program needs a model: sign in to Muse or add a key" : "Nothing leaves this Mac. The offline demo maps columns without a model; results are labelled as a demo."}
                  </p>
                  <dl>
                    <dt>Route</dt>
                    <dd>Offline demo</dd>
                  </dl>
                </>
              ) : (
                <>
                  <dl>
                    <dt>Destination</dt>
                    <dd>{preview.destination}</dd>
                    <dt>Route</dt>
                    <dd>
                      {preview.route === "api_key"
                        ? "Meta API key"
                        : "Muse plan"}
                    </dd>
                    <dt>Model</dt>
                    <dd>{preview.model}</dd>
                  </dl>
                  <span className="badge">{preview.containsRealRows ? "Contains your real example text — review before sending" : "Contains only synthetic rows"}</span>
                  {preview.webHosts.length > 0 && (
                    <p className="notice notice-info">
                      This tool will fetch from {preview.webHosts.join(", ")} at
                      run time. Every fetch is recorded under Privacy.
                    </p>
                  )}
                  <pre tabIndex={0} aria-label="Exact outbound payload">
                    {preview.payloadText}
                  </pre>
                  <p>
                    {preview.estimatedCostMicros === null
                      ? "Counts against your Muse plan"
                      : `Estimated cost: $${(preview.estimatedCostMicros / 1e6).toFixed(2)}`}{" "}
                    · Budget: ${(preview.budgetMicros / 1e6).toFixed(2)}
                  </p>
                  <p className="muted">
                    Schema and rules can still be confidential. Transmitted data
                    cannot be recalled; a canceled request may incur charges.
                    Muse receives the exact inputs above; its additional wire
                    requests are unknown.
                  </p>
                  <label className="choice">
                    <input
                      type="checkbox"
                      checked={reviewed}
                      onChange={(e) => setReviewed(e.target.checked)}
                    />
                    I reviewed what will be sent
                  </label>
                </>
              )}
              <button
                className="primary"
                disabled={
                  busy ||
                  (preview.route !== "offline_demo" && !reviewed) ||
                  status.effectiveRoute === null ||
                  !!activeBuild || (draft?.kind === "program" && preview.route === "offline_demo")
                }
                onClick={() =>
                  void act(async () => {
                    setBuild(
                      await backend.startBuild(draft!.draftId, {
                        outboundReviewed: reviewed,
                      }),
                    );
                    setStep(3);
                  })
                }
              >
                Build tool
              </button>
            </>
          )}
        </>
      )}
      {step === 3 && build && (
        <>
          <h2>Build and compare</h2>
          <p className="phase" aria-live="polite">
            Writing → Checking → Compare ·{" "}
            {build.phase === "candidate" ? "Candidate received" : build.phase}
          </p>
          <p>Building with: {build.routeLabel}</p>
          {activeBuild && (
            <button
              disabled={busy}
              onClick={() =>
                void act(async () =>
                  setBuild(await backend.stopBuild(build.buildId)),
                )
              }
            >
              Stop build
            </button>
          )}
          {build.phase === "stopped" && (
            <p role="status">
              Build stopped. Your task summary is saved. Go Back to review it
              again.
            </p>
          )}
          {build.failure && (
            <div role="alert">
              <p>{build.failure.message}</p>
              {["auth", "no_model_access"].includes(build.failure.kind) ? (
                <button onClick={openSettings}>
                  {build.failure.kind === "auth"
                    ? "Replace key"
                    : "Open model settings"}
                </button>
              ) : build.failure.kind === "quota" ? (
                <p>
                  Wait for your allowance to reset, then review and try again.
                </p>
              ) : (
                <button onClick={() => setStep(1)}>Edit rules</button>
              )}
            </div>
          )}
          {build.phase === "ready" && build.comparison && (
            <>
              <p className="notice notice-success">
                Ready to compare
                {build.route === "offline_demo" ? " · Offline demo" : ""}
              </p>
              <h3>Result</h3>
              <p>
                {build.comparison.rowsIn} rows → {build.comparison.rowsOut} rows
              </p>
              <h3>Rules</h3>
              <ul>
                {build.candidate?.output_columns.map((c) => (
                  <li key={c.name}>
                    {c.name} ← {c.source}:{" "}
                    {c.transform.type === "date"
                      ? `${formatLabels[c.transform.from]} → ${formatLabels[c.transform.to]}`
                      : c.transform.type === "money"
                        ? `${c.transform.currency}, ${c.transform.decimals} decimals`
                        : transformLabels[c.transform.type]}
                  </li>
                ))}
              </ul>
              <p>{build.program ? "Program (sandboxed)" : "Malformed values stop the run."}</p>
              <h3>Examples checked</h3>
              <ul>
                {build.checks.map((c) => (
                  <li key={c.id}>
                    {c.passed ? "✓ Pass" : "✕ Fail"} · {c.label}
                    {c.detail ? ` · ${c.detail}` : ""}
                  </li>
                ))}
              </ul>
              <h3>Needs your check</h3>
              <p>
                {build.program ? "Does this result match your task? Compare the first lines." : "Are these the right columns and date meaning? Compare the first 8 rows."}
              </p>
              <h3>Access</h3>
              <p>
                {draft?.noInput
                  ? "No input file; create one new output in the folder you choose. Existing files are never replaced."
                  : "Read the selected file; create one new output in the folder you choose. Source unchanged; existing files are never replaced."}
              </p>
              {build.program && build.program.webSources.length > 0 && (
                <>
                  <h3>Outbound fetches</h3>
                  <p>
                    This tool fetches from{" "}
                    {build.program.webSources.map((s) => hostOf(s.url)).join(", ")}{" "}
                    at run time. Runs fetch only these approved sites; a changed
                    site needs a new build and approval. Every fetch is recorded
                    under Privacy.
                  </p>
                  <label className="choice">
                    <input
                      type="checkbox"
                      checked={hostsApproved}
                      onChange={(e) => setHostsApproved(e.target.checked)}
                    />
                    I approve fetching from these sites
                  </label>
                </>
              )}
              {build.comparison.inputText !== undefined ? <><h3>Input · first lines</h3><pre>{build.comparison.inputText}</pre><h3>Output · first lines</h3><pre>{build.comparison.outputText}</pre></> : <><DataTable
                columns={build.comparison.inputColumns}
                rows={build.comparison.inputRows}
                caption="Input · first 8 rows"
              />
              <DataTable
                columns={build.comparison.outputColumns}
                rows={build.comparison.outputRows}
                caption="Staged output · first 8 rows"
              /></>}
              <Field label="Tool name">
                <input value={name} onChange={(e) => setName(e.target.value)} />
              </Field>
              <div className="actions">
                {!draft?.noInput && !(build.program && build.program.webSources.length > 0) && (
                  <button
                    disabled={busy}
                    onClick={() =>
                      void act(async () => {
                        const sample = await backend.pickCsv();
                        if (sample)
                          setBuild(
                            await backend.buildTryInput(
                              build.buildId,
                              sample.handle,
                            ),
                          );
                      })
                    }
                  >
                    Try another sample…
                  </button>
                )}
                <button onClick={() => setStep(1)}>Edit rules</button>
                <button
                  className="primary"
                  disabled={
                    busy ||
                    !name.trim() ||
                    (build.program !== null &&
                      build.program !== undefined &&
                      build.program.webSources.length > 0 &&
                      !hostsApproved)
                  }
                  onClick={() =>
                    void act(async () => {
                      await backend.saveTool(build.buildId, name);
                      await saved();
                    })
                  }
                >
                  Save tool
                </button>
              </div>
            </>
          )}
          <details>
            <summary>Advanced details</summary>
            <pre tabIndex={0}>{build.program ? build.program.code : JSON.stringify(build.candidate, null, 2)}</pre>
            <ol>
              {build.events.map((e) => (
                <li key={e.seq}>
                  {e.phase}: {e.message}
                </li>
              ))}
            </ol>
          </details>
        </>
      )}
    </section>
  );
}

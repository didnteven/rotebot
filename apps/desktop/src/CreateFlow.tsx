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
    [description, setDescription] = useState(
      initialInput
        ? "Clean this orders CSV for import: Order, Date, Amount."
        : "",
    );
  const [draft, setDraft] = useState<TaskDraft | null>(null),
    [map, setMap] = useState<CsvMapping | null>(null),
    [outputName, setOutputName] = useState("orders_ready.csv");
  const [preview, setPreview] = useState<OutboundPreview | null>(null),
    [reviewed, setReviewed] = useState(false),
    [build, setBuild] = useState<BuildState | null>(null),
    [name, setName] = useState("Clean import CSV");
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
    const next = await backend.updateMapping(draft!.draftId, map!, outputName);
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
          <h2>Describe</h2>
          <Field label="What do you do with this CSV?">
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
                  if (next) setInput(next);
                })
              }
            >
              Choose CSV…
            </button>
            <button
              disabled={busy}
              onClick={() =>
                void act(async () => setInput(await backend.loadSampleCsv()))
              }
            >
              Use the sample CSV
            </button>
          </div>
          {input && <CsvView preview={input} />}
          <button
            className="primary"
            disabled={busy || !input || !description.trim()}
            onClick={() =>
              void act(async () => {
                acceptDraft(
                  await backend.draftTask(input!.handle, description),
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
          <div
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
          </div>
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
                      You mentioned {c.name}. Connect it in Connections so later
                      tools can use it.
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
                    Nothing leaves this Mac. The offline demo maps columns
                    without a model; results are labelled as a demo.
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
                  <span className="badge">Contains only synthetic rows</span>
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
                  !!activeBuild
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
              <p>Malformed values stop the run.</p>
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
                Are these the right columns and date meaning? Compare the first
                8 rows.
              </p>
              <h3>Access</h3>
              <p>
                Read the selected CSV; create one new CSV in the folder you
                choose. Source unchanged; existing files are never replaced.
              </p>
              <DataTable
                columns={build.comparison.inputColumns}
                rows={build.comparison.inputRows}
                caption="Input · first 8 rows"
              />
              <DataTable
                columns={build.comparison.outputColumns}
                rows={build.comparison.outputRows}
                caption="Staged output · first 8 rows"
              />
              <Field label="Tool name">
                <input value={name} onChange={(e) => setName(e.target.value)} />
              </Field>
              <div className="actions">
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
                <button onClick={() => setStep(1)}>Edit rules</button>
                <button
                  className="primary"
                  disabled={busy || !name.trim()}
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
            <pre tabIndex={0}>{JSON.stringify(build.candidate, null, 2)}</pre>
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

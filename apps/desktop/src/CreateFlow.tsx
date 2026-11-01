import { BotAppearanceMenu } from "./BotAppearanceMenu";
import { CustomBotStudio } from "./CustomBotStudio";
import { BotAvatar, BotStatus, botIdentity, saveBotIdentity } from "./Bot";
import { SourceReview, SourceTechnicalDetails } from "./SourceReview";
import { ApiAccessPanel } from "./ApiAccess";
import { useEffect, useRef, useState } from "react";
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
  ToolDetail,
  Transform,
} from "./backend/types";
import { connectOffers, readyConnectors } from "./mentionedServices";
import { CsvView, DataTable, Field, FileNameInput, FailureNotice, friendlyFailure, ModelCostLines, rowCount, useAction, useScreenFocus } from "./ui";

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
/** A readable default tool name from the first sentence of the task, cut at a word. */
function toolNameFromGoal(goal: string): string {
  const tidy = (s: string) => s.replace(/[\s:;,\-–]+$/, "");
  const first = tidy(goal.trim().split(/[.\n]/)[0].trim());
  const chars = Array.from(first);
  if (chars.length <= 48) return first || "New tool";
  const cut = chars.slice(0, 48).join("");
  return `${tidy(cut.slice(0, cut.lastIndexOf(" ") > 0 ? cut.lastIndexOf(" ") : 48))}…`;
}

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
  initialGoal,
  initialDraft,
  startAtRequest = false,
  editTool,
  status,
  connectors,
  openSettings,
  connect,
  close,
  saved,
  connectionsChanged,
}: {
  backend: Backend;
  initialInput: CsvPreview | null;
  initialGoal?: string;
  initialDraft?: TaskDraft;
  /** Show the request first (filled in from initialDraft) so it goes through the same steps as a typed one. */
  startAtRequest?: boolean;
  /** A saved tool being edited: its description is the starting point, and saving adds a version to it. */
  editTool?: ToolDetail;
  status: ModelAccessStatus;
  connectors: ConnectorInfo[];
  openSettings: () => void;
  connect: (id: string) => void;
  close: () => void;
  saved: () => Promise<void>;
  connectionsChanged?: () => Promise<void>;
}) {
  const [customizingBot, setCustomizingBot] = useState(false);
  const [botSeed, setBotSeed] = useState(() => editTool ? botIdentity(editTool.id) : `draft-${globalThis.crypto.randomUUID()}`);
  const [botAppearanceSaved, setBotAppearanceSaved] = useState(false);
  function chooseBot(seed: string) {
    setBotSeed(seed);
    if (editTool) {
      saveBotIdentity(editTool.id, seed);
      setBotAppearanceSaved(true);
    }
  }
  const [step, setStep] = useState(initialDraft && !startAtRequest ? 1 : 0),
    [input, setInput] = useState(initialInput),
    [noInput, setNoInput] = useState(editTool ? editTool.noInput : !initialInput),
    [description, setDescription] = useState(
      initialGoal ?? editTool?.goal ?? (initialInput && initialInput.text === undefined
        ? "Clean this orders CSV for import: Order, Date, Amount."
        : ""),
    );
  const [draft, setDraft] = useState<TaskDraft | null>(startAtRequest ? null : initialDraft ?? null),
    [map, setMap] = useState<CsvMapping | null>(initialDraft?.mapping ?? null),
    [outputName, setOutputName] = useState(initialDraft?.outputName ?? "orders_ready.csv");
  const [preview, setPreview] = useState<OutboundPreview | null>(null),
    [build, setBuild] = useState<BuildState | null>(null),
    [name, setName] = useState(initialDraft ? toolNameFromGoal(initialDraft.goal) : editTool?.name ?? "Clean import CSV");
  const [example, setExample] = useState(initialDraft?.exampleOutput ?? "");
  const [editing, setEditing] = useState(false);
  const requestRef = useRef<HTMLTextAreaElement>(null);
  const headingRef = useScreenFocus<HTMLHeadingElement>(step);
  const [planBlockers, setPlanBlockers] = useState<string[]>([]);
  const [accessBusy, setAccessBusy] = useState(false);
  /** Connections or APIs picked before the plan is drafted. */
  // Editing a saved tool starts with the connections it already reads.
  const [chosen, setChosen] = useState<string[]>(() => [...new Set([...(editTool?.program?.connectionSources ?? []).map(s => s.connectorId), ...(initialDraft?.requiredConnections ?? []).map(s => s.connectorId)])].slice(0, 5));
  // A recording's draft is used as-is when its request is sent unchanged, so it is not made twice.
  const pendingDraft = useRef(startAtRequest ? initialDraft ?? null : null);
  const [learning, setLearning] = useState<string | null>(null);
  const usable = connectors.filter(c => c.state.kind === "connected");
  const [reviewError, setReviewError] = useState<string | null>(null);
  const [reviewAttempt, setReviewAttempt] = useState(0);
  const reviewQueue = useRef(Promise.resolve());
  const preparedAccess = useRef("");
  const accessIdentity = JSON.stringify([status.effectiveRoute, status.selectedRoute, status.egress]);
  const { act, busy, error, retry, setError } = useAction();
  const buildId = build?.buildId;
  const initialGoalStarted = useRef(false);
  useEffect(() => {
    if (!initialGoal || initialDraft || initialGoalStarted.current) return;
    initialGoalStarted.current = true;
    void act(async () => { await newDraft(() => backend.draftTaskNoInput(initialGoal)); setStep(1); });
  }, [backend, initialGoal, initialDraft]);
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
  // Preparing a review stays local. Only the Send and build action consents
  // to sending it. Discard stale reviews when any reviewed value changes.
  useEffect(() => {
    if (step !== 1 || !draft || !map) return;
    let active = true;
    setPreview(null);
    setReviewError(null);
    reviewQueue.current = reviewQueue.current.catch(() => {}).then(async () => {
      if (!active) return;
      const next = draft.kind === "program"
        ? await backend.updateTask(draft.draftId, "program", outputName, example || null)
        : await backend.updateMapping(draft.draftId, map, outputName);
      if (!active) return;
      setPlanBlockers(next.blockers);
      if (next.readyToBuild) {
        const nextPreview = await backend.outboundPreview(next.draftId);
        if (active) {
          preparedAccess.current = accessIdentity;
          setPreview(nextPreview);
        }
      }
    }).catch(error => {
      if (active) {
        setReviewError(error instanceof Error ? error.message : "Couldn't prepare the plan.");
        setPlanBlockers(["Couldn't prepare the plan. Try again or change the request."]);
      }
    });
    return () => { active = false; };
  }, [step, backend, draft, map, outputName, example, status.effectiveRoute, status.selectedRoute, status.egress, reviewAttempt]);
  useEffect(() => {
    if (step === 1 && editing) requestRef.current?.focus();
  }, [step, editing]);
  function acceptDraft(next: TaskDraft) {
    setDraft(next);
    setName(editTool ? editTool.name : next.kind === "program" ? toolNameFromGoal(next.goal) : "Clean import CSV");
    setMap(next.mapping);
    setOutputName(next.outputName);
    setPreview(null);
    setPlanBlockers(next.blockers);
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
  /** Every draft made while editing a saved tool is marked, so saving adds a version to that tool. */
  async function newDraft(make: () => Promise<TaskDraft>) {
    let next = await make();
    if (editTool) next = await backend.setDraftReplaces(next.draftId, editTool.id);
    // A service the description mentions that is already connected is switched on for this tool
    // (read-only; the exact reads are still approved before anything is read).
    const ready = readyConnectors(next.mentionedConnectors, connectors, (next.requiredConnections ?? []).map(s => s.connectorId));
    const picked = chosen.filter(id => usable.some(c => c.id === id));
    const ids = [...new Set([...(next.requiredConnections ?? []).map(s => s.connectorId), ...picked, ...ready])];
    if (ids.length > (next.requiredConnections ?? []).length) next = await backend.selectTaskConnections(next.draftId, ids.slice(0, 5));
    // A custom API can return anything: read the paths this task needs once (field names only), so the
    // program is written for what the service really returns.
    const apis = (next.requiredConnections ?? []).filter(s => connectors.some(c => c.id === s.connectorId && c.registration === "api"));
    if (apis.length && status.effectiveRoute && status.effectiveRoute !== "offline_demo") {
      setLearning(`Learning what ${apis.map(s => s.name).join(" and ")} returns…`);
      try {
        for (const s of apis) await backend.learnApiForTask(s.connectorId, next.goal).catch(() => []);
        next = await backend.selectTaskConnections(next.draftId, (next.requiredConnections ?? []).map(s => s.connectorId));
      } finally { setLearning(null); }
    }
    acceptDraft(next);
    if (editTool?.program) setOutputName(editTool.program.output_name);
  }
  async function describe() {
    const pending = pendingDraft.current;
    pendingDraft.current = null;
    await newDraft(() => pending && noInput && description.trim() === pending.goal ? Promise.resolve(pending)
      : noInput ? backend.draftTaskNoInput(description)
      : backend.draftTask(input!.handle, description));
    setEditing(false);
    setStep(1);
  }
  async function sendAndBuild() {
    if (!draft || !preview || description.trim() !== draft.goal) return;
    if (preparedAccess.current !== accessIdentity || status.effectiveRoute === null) {
      setStep(1);
      return;
    }
    setBuild(await backend.startBuild(draft.draftId, { outboundReviewed: true }));
    setStep(2);
  }
  function changeRequest() {
    setError(null);
    if (!draft) {
      setStep(0);
      requestRef.current?.focus();
      return;
    }
    setEditing(true);
    setStep(1);
  }
  // Read-only connections the person chose for this task (on the first step or the plan) are already
  // approved: a build that only reads them starts testing without asking again. A model step still asks.
  const autoApproved = useRef<string | null>(null);
  useEffect(() => {
    if (build?.phase !== "candidate" || !build.program || build.program.modelStep || autoApproved.current === build.buildId) return;
    const sources = build.program.connectionSources ?? [];
    const chosenIds = new Set((draft?.requiredConnections ?? []).map(c => c.connectorId));
    if (!sources.length || !sources.every(s => chosenIds.has(s.connectorId))) return;
    autoApproved.current = build.buildId;
    void act(async () => setBuild(await backend.approveBuildSources(build.buildId, build.program!)));
  }, [build, draft, backend]);
  const activeBuild = build && !terminal(build);
  const sites = [...new Set(build?.program?.webSources.map(s => hostOf(s.url)) ?? [])];
  const blocked = busy || accessBusy ? "Working…" : activeBuild ? "Stop the current build before sending another request."
    : status.effectiveRoute === null ? "Choose a model in Settings before building."
    : draft?.kind === "program" && preview?.route === "offline_demo" ? "Writing this tool needs a model: sign in to Muse or add a key in Settings."
    : draft && description.trim() !== draft.goal ? "Update the plan to use your changed request."
    : planBlockers.length ? planBlockers[0]
    : !preview ? "Preparing what will be sent…" : null;
  return (
    <section aria-label="Create tool">
      {customizingBot && <CustomBotStudio backend={backend} status={status} currentSeed={botSeed} title={editTool?.name ?? (description.trim() ? toolNameFromGoal(description) : "")} useBot={chooseBot} close={() => setCustomizingBot(false)} openSettings={openSettings} />}
      <div className="actions create-nav">
        <button className="quiet" disabled={busy} onClick={() => { if (step === 0) close(); else setStep(step - 1); }}>
          <span aria-hidden="true">‹ </span>Back
        </button>
        <button className="quiet" disabled={busy || !!activeBuild} onClick={close}>Back to My tools</button>
      </div>
      <ol className="stepper" aria-label="Create progress">
        {[
          "What should it do?",
          "Check the plan",
          "Result",
        ].map((label, i) => (
          <li key={label} aria-current={i === step ? "step" : undefined}>
            {i + 1}. {label}
          </li>
        ))}
      </ol>
      {error && <FailureNotice message={error} retry={() => void act(retry)} change={changeRequest} />}
      {activeBuild && step !== 2 && <div className="notice notice-info">
        <p>A build is in progress.</p>
        <button onClick={() => setStep(2)}>Return to result</button>
        <button disabled={busy} onClick={() => void act(async () => setBuild(await backend.stopBuild(build.buildId)))}>Stop build</button>
      </div>}

      {busy && !activeBuild && <BotStatus seed={botSeed} label={learning ?? (step === 0 ? "Preparing your task…" : step === 1 ? "Updating your plan…" : "Preparing your tool…")} />}

      {step === 0 && botAppearanceSaved && <p role="status">Bot appearance saved. You can go back without rebuilding the task.</p>}
      {step === 0 && (
        <>
          <div className="task-description-heading">
            <BotAppearanceMenu seed={botSeed} disabled={busy} shuffle={() => chooseBot(`draft-${globalThis.crypto.randomUUID()}`)} customize={() => setCustomizingBot(true)} />
            <h2 ref={headingRef} tabIndex={-1}>What should it do?</h2>
          </div>
          <p className="muted">Describe the result you want, where the information comes from, and any rules to follow. For example: “Summarize today’s calendar and list my first meeting.”</p>
          {editTool && <p role="note" className="notice notice-info">Editing “{editTool.name}”. Change the description, then check the plan and test it. Task changes are applied only after you test and save them; the current version stays in its version history. Bot appearance saves immediately.</p>}
          {editTool && !editTool.noInput && !input && <p className="muted">This tool reads a file. Choose a sample file to test the new version.</p>}
          <Field label="What should the tool do?">
            <textarea
              ref={requestRef}
              rows={4}
              placeholder="What would you like help with?"
              value={description}
              onChange={(e) => setDescription(e.target.value)}
            />
          </Field>
          <div className="actions file-options">
            {input && !noInput
              ? <button className="quiet" disabled={busy} onClick={() => { setInput(null); setNoInput(true); }}>Remove file</button>
              : <button className="quiet" disabled={busy} onClick={() => void act(async () => {
                  const next = await backend.pickCsv();
                  if (next) { setInput(next); setNoInput(false); }
                })}>+ Add a file</button>}
          </div>
          <div className="actions source-options" role="group" aria-label="Use data from">
            <span className="muted">Use data from</span>
            {usable.map(c => {
              const on = chosen.includes(c.id);
              return <button key={c.id} className={on ? "chip chip-on" : "chip"} aria-pressed={on} disabled={busy || (!on && chosen.length >= 5)}
                onClick={() => setChosen(on ? chosen.filter(id => id !== c.id) : [...chosen, c.id])}>{on ? "✓ " : ""}{c.name}</button>;
            })}
            <button className="quiet" disabled={busy} onClick={() => connect("")}>{usable.length ? "+ Connect another" : "Connect a service or API"}</button>
          </div>
          {input && !noInput && <CsvView preview={input} />}

          {!description.trim() && <p className="muted">Describe a task to continue.</p>}
          <button
            className="primary"
            disabled={busy || (!input && !noInput) || !description.trim()}
            onClick={() => void act(describe)}
          >
            Check the plan
          </button>
        </>
      )}
      {step === 1 && draft && map && (
        <>
          <h2 ref={headingRef} tabIndex={-1}>Check the plan</h2>
          <div className="card plan-card">
            {editing ? <>
              <Field label="What should the tool do?">
                <textarea ref={requestRef} rows={4} value={description} onChange={e => setDescription(e.target.value)} />
              </Field>
              <button disabled={busy || !description.trim() || !!activeBuild} onClick={() => void act(describe)}>Update plan</button>
            </> : <>
              <p className="plan-sentence">The tool will {draft.goal.charAt(0).toLowerCase()}{draft.goal.slice(1).replace(/[.!?]+$/, "")}.</p>
              <dl className="plan-facts">
                {!!draft.requiredConnections?.length && <><dt>Data</dt><dd>Using your {draft.requiredConnections.map(s => s.name).join(", ")} connection (read-only)</dd></>}
                {preview && <><dt>Built by</dt><dd>{preview.route === "offline_demo" ? "Offline demo, nothing leaves this Mac" : preview.summary.recipient}</dd></>}
              </dl>
              <button className="quiet" onClick={() => setEditing(true)}>Change request</button>
            </>}
          </div>
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
          <details key="plan-details"><summary>Details</summary>
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
            <span>Write values that change in {"{braces}"}, for example Hottest: {"{city}"} {"{temp}"}. This is your real text. It will be included when you send the plan.</span>
          </Field>}
<ul>{draft.summary.map(line => <li key={line}>{line}</li>)}</ul>
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
          {planBlockers.length > 0 && (
            <div className="notice notice-warning">
              <p>Before you build</p>
              <ul>
                {planBlockers.map((b) => (
                  <li key={b}>{b}</li>
                ))}
              </ul>
            </div>
          )}
          {(connectors.some(c => c.state.kind === "connected") || !!draft.requiredConnections?.length) && <fieldset disabled={busy || !!activeBuild}>
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
          </fieldset>}
          </details>
          {draft.kind === "program" && !editing && !activeBuild && !(chosen.length && draft.requiredConnections?.length && !draft.accessPlan) && <ApiAccessPanel key={draft.draftId} backend={backend} draft={draft} status={status} accept={acceptDraft} changed={connectionsChanged} onBusy={setAccessBusy} connect={connect} connectors={connectors} />}
          {connectOffers(draft.mentionedConnectors, connectors)
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
          {reviewError && <FailureNotice message={reviewError} retry={() => setReviewAttempt(attempt => attempt + 1)} change={changeRequest} />}
          {status.effectiveRoute === null && <div className="notice notice-warning"><p>Choose a model in Settings to build this tool.</p><button onClick={openSettings}>Open model settings</button></div>}
          {preview && <details key="send-details" className="send-review">
            <summary>What leaves this Mac · {preview.route === "offline_demo" ? "Nothing" : preview.summary.recipient}</summary>
            {preview.route === "offline_demo" && <p className="notice notice-info">Nothing leaves this Mac. The offline demo maps columns without a model; results are labelled as a demo.</p>}
            <dl><dt>Recipient</dt><dd>{preview.summary.recipient}</dd><dt>Model</dt><dd>{preview.summary.model}</dd><dt>Cost</dt><dd>{preview.summary.cost}</dd></dl>
            <h3>What is included</h3>
            <ul>{preview.summary.includes.map((item, i) => <li key={i}>{item.label}{item.real && <span className="badge">Your real text</span>}{item.detail && <p className="preserve-lines">{item.detail}</p>}</li>)}</ul>
            <h3>Later runs</h3><ul>{preview.summary.laterRuns.map((line, i) => <li key={i}>{line}</li>)}</ul>
            <details><summary>Exact data sent</summary><pre tabIndex={0} aria-label="Exact data sent">{preview.payloadText}</pre></details>
          </details>}
          {blocked && <p id="build-reason" role="status">{blocked}{draft && !busy && !accessBusy && description.trim() !== draft.goal && <> <button className="quiet" onClick={() => void act(describe)}>Update the plan</button></>}</p>}
          <div className="build-bar">
            <button className="primary" disabled={!!blocked} aria-describedby={blocked ? "build-reason" : "build-consent"} onClick={() => void act(sendAndBuild)}>Send and build</button>
            {!blocked && <span id="build-consent" className="muted">Sends what is listed above{preview && preview.route !== "offline_demo" ? ` to ${preview.summary.recipient}` : ""} and tests the tool.</span>}
          </div>
        </>
      )}
      {step === 2 && build && (
        <>
          <h2 ref={headingRef} tabIndex={-1}>Result</h2>
          {!activeBuild && <BotAvatar seed={botSeed} state={build.phase === "ready" ? "success" : build.phase === "failed" ? "error" : "paused"} size={96} />}
          {activeBuild && <BotStatus large seed={botSeed} state={build.phase === "candidate" ? "waiting" : build.phase === "checking" ? "working" : "building"} label={build.phase === "candidate" ? "Check access before testing." : build.phase === "checking" ? "Checking your tool…" : "Building your tool…"} detail={build.phase === "candidate" ? "Your bot is waiting for you to review access." : "You can stop the build at any time."} />}
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
          {build.phase === "candidate" && build.program && autoApproved.current !== build.buildId && ((build.program.connectionSources?.length??0)>0 || build.program.modelStep) && <div className="card">
            <h3>Allow access to test this tool</h3>
            <SourceReview advanced={false} program={build.program} route={build.route} routeDetails={build.routeLabel} build connections={draft?.requiredConnections}/>
            <button className="primary" disabled={busy} onClick={()=>void act(async()=>setBuild(await backend.approveBuildSources(build.buildId,build.program!)))}>Approve reads and test</button>
          </div>}
          {build.phase === "stopped" && (
            <p role="status">
              Build stopped. Your plan is saved. Change the request or try again.
            </p>
          )}
          {(build.failure || build.phase === "stopped") && <div role="alert" className="notice notice-warning">
            <p>{build.failure ? friendlyFailure(build.failure.message, build.failure.kind) : "Build stopped. Your plan is saved."}</p>
            {build.failure?.kind === "missing_data"
              ? <div className="actions">
                  <button className="primary" onClick={() => { setError(null); setEditing(false); setStep(1); }}>Check how to get this data</button>
                  <button onClick={() => { setError(null); setStep(0); }}>Use a file instead</button>
                  <button onClick={changeRequest}>Change the request</button>
                </div>
              : <div className="actions"><button className="primary" disabled={busy} onClick={() => void act(sendAndBuild)}>Try again</button><button onClick={changeRequest}>Change the request</button></div>}
            {build.failure && ["auth", "no_model_access"].includes(build.failure.kind) && <button onClick={openSettings}>Open model settings</button>}
          </div>}
          {build.phase === "ready" && build.comparison && (
            <>
              {build.comparison.outputText !== undefined ? <div className={`result-answer preserve-lines${build.comparison.outputText.length>200?" long":""}`} tabIndex={0} aria-label="Answer">{build.comparison.outputText}</div> : <DataTable columns={build.comparison.outputColumns} rows={build.comparison.outputRows} caption="Result · first 8 rows" />}
              {!draft?.noInput && !sites.length && <p>{rowCount(build.comparison.rowsIn)} → {rowCount(build.comparison.rowsOut)}</p>}
              {build.route === "offline_demo" && <p className="muted">Offline demo</p>}
              {build.testModelCost && <ModelCostLines cost={build.testModelCost} />}
              <p>The tool creates a new output file in the folder you choose. Existing files stay unchanged.</p>
              {build.program && <SourceReview advanced={false} program={build.program} route={build.route} routeDetails={build.routeLabel} connections={draft?.requiredConnections} />}
              <Field label="Tool name">
                <input value={name} onChange={(e) => setName(e.target.value)} />
              </Field>
              {editTool && <p role="note" className="notice notice-info">Saving adds a new version of “{editTool.name}”. The current version stays in its version history and can be restored.{editTool.noInput ? " If this tool is scheduled, you will be asked to approve the new version before it runs on its own." : ""}</p>}
              <h3>Is this right?</h3>
              {!!sites.length && <p id="save-approval" className="notice notice-info">This tool will read data from {sites.join(", ")} each time it runs. Allow? Press “{editTool ? "Yes, save changes" : "Yes, save this tool"}” to allow this access and save.</p>}

              {!name.trim() && <p id="save-reason">Give the tool a name before saving.</p>}
              <div className="actions">
                <button className="primary" disabled={busy || !name.trim()} aria-describedby={sites.length ? "save-approval" : !name.trim() ? "save-reason" : undefined} onClick={() => void act(async () => { const savedTool = await backend.saveTool(build.buildId, name); saveBotIdentity(savedTool.id, botSeed); await saved(); })}>{editTool ? "Yes, save changes" : "Yes, save this tool"}</button>
                <button onClick={changeRequest}>No, change it</button>
              </div>
            </>
          )}
          <details>
            <summary>Details</summary>
          {build.events.some(e=>/to review the output/i.test(e.message)) && !build.events.some(e=>/Checks failed/i.test(e.message)) ? <p role="status">The checks passed, so Macroscope had the model double-check the output against the page it read.</p> : build.events.some(e=>/correct|fix/i.test(e.message)) && <p role="status">{(build.events.some(e=>e.message.includes("(example format)")) || build.checks.some(c=>!c.passed && /example|shape|format/i.test(c.id))) ? "The output didn’t match your example’s format — Macroscope asked the model to fix it." : "The tool did not pass its checks — Macroscope asked the model to fix it."}</p>}
          {build.checks.some(c=>!c.passed && /example|shape|format/i.test(c.id)) && <p role="status">Tip: write values that change in {"{braces}"}, like {"{city}"} or {"{temp}"}.</p>}
            {build.failure && <p>{build.failure.message}</p>}
            {(build.program || build.candidate) && <p>{build.program ? "Program (sandboxed)" : "CSV mapping"}</p>}
            {build.candidate && <ul>{build.candidate.output_columns.map(c => <li key={c.name}>{c.name} ← {c.source}: {c.transform.type}</li>)}</ul>}
            {!draft?.noInput && build.comparison && <>{build.comparison.inputText !== undefined ? <><h3>Input · first lines</h3><pre>{build.comparison.inputText}</pre></> : <DataTable columns={build.comparison.inputColumns} rows={build.comparison.inputRows} caption="Input · first 8 rows" />}</>}
            {build.phase === "ready" && !draft?.noInput && !sites.length && <button disabled={busy} onClick={() => void act(async () => { const sample = await backend.pickCsv(); if (sample) setBuild(await backend.buildTryInput(build.buildId, sample.handle)); })}>Try another sample</button>}
            <ul>{build.checks.map(c=><li key={c.id}>{c.passed ? "✓ Pass" : "✕ Fail"} · {c.label}{c.detail ? ` · ${c.detail}` : ""}</li>)}</ul>
            {build.program && <SourceTechnicalDetails program={build.program}/>}
            {(build.program || build.candidate) && <pre tabIndex={0}>{build.program ? build.program.code : JSON.stringify(build.candidate, null, 2)}</pre>}
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

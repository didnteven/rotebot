import { BotAppearanceMenu } from "./BotAppearanceMenu";
import { BuildProgress } from "./BuildProgress";
import { CustomBotStudio } from "./CustomBotStudio";
import { BotAvatar, BotStatus, botIdentity, saveBotIdentity } from "./Bot";
import { SourceReview, SourceTechnicalDetails } from "./SourceReview";
import { ApiAccessPanel } from "./ApiAccess";
import { Icon, serviceIcon } from "./Icon";
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
import { CsvView, DataTable, Field, FileNameInput, FileText, FailureNotice, friendlyFailure, hostOf, ModelCostLines, rowCount, useAction, useScreenFocus, usedParts, UsedLine } from "./ui";

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
  const requestRef = useRef<HTMLTextAreaElement>(null);
  const headingRef = useScreenFocus<HTMLHeadingElement>(step);
  const focusRequest = useRef(false);
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
    if (step === 0 && focusRequest.current) requestRef.current?.focus();
    focusRequest.current = false;
  }, [step]);
  const aiReady = status.egress === "cloud_allowed" && !!status.effectiveRoute && status.effectiveRoute !== "offline_demo";
  const suggested = useRef<string | null>(null);
  const buildStarted = useRef(false);
  /** A name the person typed is kept over any suggestion. */
  const nameEdited = useRef(false);
  buildStarted.current = !!build;
  const [suggesting, setSuggesting] = useState(false);
  /** The AI wrote no layout this time, so the empty box is explained rather than left blank. */
  const [layoutMissed, setLayoutMissed] = useState(false);
  useEffect(() => {
    if (step !== 1 || !draft || draft.kind !== "program" || (example.trim() && draft.suggestedName) || !aiReady || build || suggested.current === `${draft.draftId}:${draft.goal}`) return;
    const key = `${draft.draftId}:${draft.goal}`;
    suggested.current = key;
    const id = draft.draftId;
    setSuggesting(true);
    setLayoutMissed(false);
    void backend.suggestExampleOutput(id)
      // A build already sent keeps its plan: a late layout never changes it.
      .then(next => {
        // The plan this layout was written for has since been discarded.
        if (suggested.current !== key) return;
        if (next.suggestedName && !nameEdited.current && !editTool) setName(next.suggestedName);
        if (next.exampleOutput && !buildStarted.current) setExample(current => current.trim() ? current : next.exampleOutput!);
        if (!next.exampleOutput) setLayoutMissed(true);
      })
      // The person can still type the layout themselves.
      .catch(() => { if (suggested.current === key) setLayoutMissed(true); })
      .finally(() => { if (suggested.current === key) setSuggesting(false); });
  }, [step, draft, example, aiReady, backend, build]);
  /** What Clear removed, so Undo can put the task back exactly. */
  const [clearedTask, setClearedTask] = useState<{ description: string; draft: TaskDraft | null; map: CsvMapping | null; build: BuildState | null; example: string; name: string; outputName: string; preview: OutboundPreview | null; chosen: string[] } | null>(null);
  const planExists = !!draft || !!build;
  /** Connections a saved tool already reads are kept; ones picked for a discarded plan are not. */
  const toolConnections = () => [...new Set((editTool?.program?.connectionSources ?? []).map(s => s.connectorId))].slice(0, 5);
  /** Discards the plan and everything made from it, because all of it depends on the request. */
  function discardPlan() {
    setDraft(null);
    setMap(null);
    setBuild(null);
    setExample("");
    setPreview(null);
    setPlanBlockers([]);
    setLayoutMissed(false);
    setSuggesting(false);
    setReviewError(null);
    setError(null);
    nameEdited.current = false;
    suggested.current = null;
    pendingDraft.current = null;
  }
  function changeDescription(value: string) {
    if (planExists) {
      discardPlan();
      setChosen(toolConnections());
    }
    setDescription(value);
    setClearedTask(null);
  }
  /** Starts the request over: the text and the plan, result, layout and connections made from it. */
  function clearTask() {
    setClearedTask({ description, draft, map, build, example, name, outputName, preview, chosen });
    setDescription("");
    discardPlan();
    setChosen(toolConnections());
    requestRef.current?.focus();
  }
  function undoClear() {
    if (!clearedTask) return;
    setDescription(clearedTask.description);
    setDraft(clearedTask.draft);
    setMap(clearedTask.map);
    setBuild(clearedTask.build);
    setExample(clearedTask.example);
    setName(clearedTask.name);
    setOutputName(clearedTask.outputName);
    setPreview(clearedTask.preview);
    setChosen(clearedTask.chosen);
    setClearedTask(null);
    requestRef.current?.focus();
  }
  function acceptDraft(next: TaskDraft) {
    setDraft(next);
    if (!nameEdited.current) setName(editTool ? editTool.name : next.kind === "program" ? next.suggestedName || toolNameFromGoal(next.goal) : "Clean import CSV");
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
    // Nothing changed since this plan was made, so its output and connections still apply.
    if (draft && description.trim() === draft.goal) {
      setStep(1);
      return;
    }
    if (draft) discardPlan();
    const pending = pendingDraft.current;
    pendingDraft.current = null;
    await newDraft(() => pending && noInput && description.trim() === pending.goal ? Promise.resolve(pending)
      : noInput ? backend.draftTaskNoInput(description)
      : backend.draftTask(input!.handle, description));
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
    if (step === 0) requestRef.current?.focus();
    else focusRequest.current = true;
    setStep(0);
  }
  // Build and test is the go-ahead for the reads and the AI step a build tests with: the person
  // accepted their AI service and connections once, and the result lists what was used.
  const autoApproved = useRef<string | null>(null);
  useEffect(() => {
    // Only connection reads and an AI step wait for this go-ahead. A program that reads public web
    // pages only has already moved on, so approving it would fail and show a false failure.
    // A correction can add an AI step to a build approved without one, and that needs its own go-ahead.
    const approvalKey = `${build?.buildId}|${build?.program?.modelStep?.prompt ?? ""}`;
    if (build?.phase !== "candidate" || !build.program || autoApproved.current === approvalKey
      || (!build.program.connectionSources?.length && !build.program.modelStep)) return;
    autoApproved.current = approvalKey;
    void act(async () => setBuild(await backend.approveBuildSources(build.buildId, build.program!)));
  }, [build, draft, backend]);
  const activeBuild = build && !terminal(build);
  const includedConnections = draft ? (draft.requiredConnections ?? []).map(s => s.connectorId) : chosen;
  const sites = [...new Set(build?.program?.webSources.map(s => hostOf(s.url)) ?? [])];
  const blocked = busy || accessBusy ? "Working…" : activeBuild ? "Stop the current build before sending another request."
    : status.effectiveRoute === null ? "Choose a model in Settings before building."
    : draft?.kind === "program" && preview?.route === "offline_demo" ? "Writing this tool needs a model: sign in to Muse or add a key in Settings."
    : draft && description.trim() !== draft.goal ? "Update the plan to use your changed request."
    : suggesting && !example.trim() ? "Writing the output layout…"
    : planBlockers.length ? planBlockers[0]
    : !preview ? "Preparing what will be sent…" : null;
  return (
    <section className="create-flow" aria-label="Create tool">
      {customizingBot && <CustomBotStudio backend={backend} status={status} currentSeed={botSeed} title={editTool?.name ?? (description.trim() ? toolNameFromGoal(description) : "")} useBot={chooseBot} close={() => setCustomizingBot(false)} openSettings={openSettings} />}
      <div className="create-flow-navigation">
      <div className="actions create-nav">
        <button className="quiet" disabled={busy} onClick={() => { if (step === 0) close(); else setStep(step - 1); }}>
          <span aria-hidden="true">‹ </span>Back
        </button>
      </div>
      <ol className="stepper create-stepper" aria-label="Create progress">
        {[
          "Describe",
          "Review",
          "Build & test",
        ].map((label, i) => (
          <li key={label} className={i < step ? "step-complete" : undefined} aria-current={i === step ? "step" : undefined}>
            <span className="create-step-number" aria-hidden="true">{i < step ? "✓" : String(i + 1).padStart(2, "0")}</span><span>{label}</span>
          </li>
        ))}
      </ol>
      </div>
      {error && <FailureNotice message={error} retry={() => void act(retry)} change={changeRequest} />}
      {activeBuild && step !== 2 && <div className="notice notice-info">
        <p>A build is in progress.</p>
        <button onClick={() => setStep(2)}>Return to result</button>
        <button disabled={busy} onClick={() => void act(async () => setBuild(await backend.stopBuild(build.buildId)))}>Stop build</button>
      </div>}

      {busy && !activeBuild && <BotStatus seed={botSeed} label={learning ?? (step === 0 ? "Preparing your task…" : step === 1 ? "Updating your plan…" : "Preparing your tool…")} />}

      {step === 0 && botAppearanceSaved && <p role="status">Character appearance saved. You can go back without rebuilding the task.</p>}
      {step === 0 && (
        <div className="task-composer">
          <div className="composer-heading">
            <div className="composer-portrait"><BotAppearanceMenu size={112} seed={botSeed} disabled={busy} shuffle={() => chooseBot(`draft-${globalThis.crypto.randomUUID()}`)} customize={() => setCustomizingBot(true)} /></div>
            <div><h2 ref={headingRef} tabIndex={-1}>What should it do?</h2><p>Describe the job. Your bot will turn it into a tool you can review and run again.</p></div>
          </div>
          <div className="composer-request">
          {editTool && <p role="note" className="muted">Editing “{editTool.name}”. Your current version is kept until you save.</p>}
          {editTool && !editTool.noInput && !input && <p className="muted">This tool reads a file. Choose a sample file to test the new version.</p>}
          {draft && <p role="note" className="muted">Changing anything here starts a new plan, so the output and connections are set up again.</p>}
          <textarea
            aria-label="What should the tool do?"
            ref={requestRef}
            rows={4}
            placeholder="For example: Summarize today’s calendar and list my first meeting"
            value={description}
            readOnly={busy || !!activeBuild}
            onChange={(e) => changeDescription(e.target.value)}
          />
          {description.trim()
            ? <div className="actions task-clear"><button className="ghost" disabled={busy || !!activeBuild} onClick={clearTask}>Clear</button></div>
            : clearedTask && <p role="status" className="muted task-clear">Task cleared. <button className="ghost" onClick={undoClear}>Undo</button></p>}
          </div>
          <div className="composer-sources">
          <div className="source-section-heading"><h3>Files and services</h3><span>Optional</span></div>
          <p className="composer-source-help">Choose only what this task needs. Otherwise, leave this empty.</p>
          {input && !noInput
            ? <div className="source-file-attached"><Icon name="files" /><div><strong><FileText text={input.handle.displayName} /></strong><span>Attached to this task</span></div><button className="ghost" disabled={busy || !!activeBuild} onClick={() => { if (planExists) discardPlan(); setInput(null); setNoInput(true); }}>Remove file</button></div>
            : <button className="source-file-picker" aria-label="+ Add a file" disabled={busy || !!activeBuild} onClick={() => void act(async () => {
                const next = await backend.pickCsv();
                if (next) { if (planExists) discardPlan(); setInput(next); setNoInput(false); }
              })}><span className="source-file-icon"><Icon name="files" /></span><span><strong>Attach a file</strong><span>Choose a file from your Mac</span></span><span className="source-file-plus" aria-hidden="true">+</span></button>}
          <div className="source-service-heading"><h4>Connected services</h4><button className="ghost" disabled={busy} onClick={() => connect("")}>Manage connections</button></div>
          {!!usable.length && <div className="source-service-list" role="group" aria-label="Use data from">
            {usable.map(c => {
              const on = includedConnections.includes(c.id);
              return <button key={c.id} className={`source-service${on ? " source-service-selected" : ""}`} aria-label={c.name} aria-pressed={on} disabled={busy || !!activeBuild || (!on && includedConnections.length >= 5)}
                onClick={() => { if (planExists) discardPlan(); setChosen(on ? includedConnections.filter(id => id !== c.id) : [...includedConnections, c.id]); }}>
                <span className="source-service-icon"><Icon name={serviceIcon(c)} /></span><span className="source-service-copy"><strong>{c.name}</strong><span>{on ? "Included in this task" : "Select to include"}</span></span><span className="source-select-mark" aria-hidden="true">{on ? "✓" : ""}</span>
              </button>;
            })}
          </div>}
          {!usable.length && <p className="source-empty">No services connected yet. You can still describe your task above.</p>}
          {includedConnections.length >= 5 && <p className="composer-source-help" role="status">You can include up to five services. Deselect one to choose another.</p>}
          {input && !noInput && <CsvView preview={input} />}
          </div>

          <div className="composer-footer">
          {noInput && !aiReady && <div className="notice notice-info"><p>Building your own tools needs an AI service.</p><button onClick={openSettings}>Set up AI service</button></div>}
          <button
            className="primary"
            disabled={busy || (!input && !noInput) || !description.trim()}
            onClick={() => void act(describe)}
          >
            Check the plan
          </button>
          </div>
        </div>
      )}
      {step === 1 && draft && map && (
        <div className="task-composer task-review">
          <div className="composer-heading">
            <div className="composer-portrait review-portrait"><BotAvatar seed={botSeed} size={112} /></div>
            <div><h2 ref={headingRef} tabIndex={-1}>Check the plan</h2><p>Check the task, output, and sources before your bot builds the tool.</p></div>
          </div>
          <div className="review-body">
          <div className="card plan-card">
            <p className="plan-sentence">The tool will {draft.goal.charAt(0).toLowerCase()}{draft.goal.slice(1).replace(/[.!?]+$/, "")}.</p>
            <dl className="plan-facts">
              {!!draft.requiredConnections?.length && <><dt>Data</dt><dd>Using your {draft.requiredConnections.map(s => s.name).join(", ")} connection (read-only)</dd></>}
            </dl>
            <button className="quiet" onClick={changeRequest}>Change request</button>
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
          <details key="plan-output" className="output-section" open={draft.kind === "program"}><summary>Output</summary>
          {draft.kind === "program" && suggesting && !example.trim() && <BotStatus seed={botSeed} label="Writing the output layout…" detail="You can type your own instead." />}
          {draft.kind === "program" && <Field label="What the result should look like">
            <textarea aria-label="What the result should look like" aria-busy={suggesting} rows={5} maxLength={16000} value={example} placeholder={suggesting ? "Writing a suggested layout…" : "Water: {water_cost}"} onChange={(e) => setExample(e.target.value)} />
            <span className="muted">{layoutMissed && !suggesting && !example.trim()
              ? "No layout was suggested this time. Type one, or leave it empty and the AI will choose one when it builds."
              : <>Change anything. Words in {"{braces}"} are filled in each run.</>}</span>
          </Field>}
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
          <details key="plan-advanced"><summary>Advanced</summary>
          <Field label="Task kind">
            <select value={draft.kind ?? "csv-mapping"} disabled={busy || draft.noInput} onChange={(e) => void act(async () => {
              const kind = e.target.value as "csv-mapping" | "program";
              acceptDraft(await backend.updateTask(draft.draftId, kind, outputName.replace(/\.[^.]+$/, kind === "program" ? ".txt" : ".csv"), example || null));
            })}>
              <option value="csv-mapping">CSV mapping</option>
              <option value="program">Program (sandboxed)</option>
            </select>
          </Field>
<ul>{draft.summary.map(line => <li key={line}>{line}</li>)}</ul>
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
          </details>
          {draft.kind === "program" && !activeBuild && !(chosen.length && draft.requiredConnections?.length && !draft.accessPlan) && <ApiAccessPanel key={draft.draftId} backend={backend} draft={draft} status={status} accept={acceptDraft} changed={connectionsChanged} onBusy={setAccessBusy} connect={connect} connectors={connectors} />}
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
            <summary>What gets sent · {preview.route === "offline_demo" ? "Nothing" : preview.summary.recipient}</summary>
            {preview.route === "offline_demo" && <p className="notice notice-info">Nothing leaves this Mac. The offline demo maps columns without a model; results are labelled as a demo.</p>}
            <dl><dt>Recipient</dt><dd>{preview.summary.recipient}</dd><dt>Model</dt><dd>{preview.summary.model}</dd><dt>Cost</dt><dd>{preview.summary.cost}</dd></dl>
            <h3>What is included</h3>
            <ul>{preview.summary.includes.map((item, i) => <li key={i}>{item.label}{item.real && <span className="badge">Your real text</span>}{item.detail && <p className="preserve-lines">{item.detail}</p>}</li>)}</ul>
            <h3>Later runs</h3><ul>{preview.summary.laterRuns.map((line, i) => <li key={i}>{line}</li>)}</ul>
            <details><summary>Exact data sent</summary><pre tabIndex={0} aria-label="Exact data sent">{preview.payloadText}</pre></details>
          </details>}
          </div>
          <div className="composer-footer review-footer">
          {blocked && <p id="build-reason" role="status">{blocked}{draft && !busy && !accessBusy && description.trim() !== draft.goal && <> <button className="quiet" onClick={() => void act(describe)}>Update the plan</button></>}</p>}
          <div className="build-bar">
            <button className="primary" disabled={!!blocked} aria-describedby={blocked ? "build-reason" : undefined} onClick={() => void act(sendAndBuild)}>Build and test</button>
          </div>
          </div>
        </div>
      )}
      {step === 2 && build && (
        <>
          <h2 ref={headingRef} tabIndex={-1}>Result</h2>
          <BuildProgress build={build} seed={botSeed} name={name} busy={busy}
            stop={() => void act(async () => setBuild(await backend.stopBuild(build.buildId)))} />
          {(build.failure || build.phase === "stopped") && <div role="alert" className="notice notice-warning">
            <p>{build.failure ? friendlyFailure(build.failure.message, build.failure.kind) : "Build stopped. Your plan is saved."}</p>
            {build.failure?.kind === "missing_data"
              ? <div className="actions">
                  <button className="primary" onClick={() => { setError(null); setStep(1); }}>Check how to get this data</button>
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
              <UsedLine parts={usedParts(build.program, build.route, draft?.requiredConnections)} />
              {draft?.noInput && build.program && !build.program.webSources.length && !build.program.connectionSources?.length && !build.program.modelStep
                && <p className="muted" role="note">This tool gives the same result every run; only the date can change. If you want something new each time, choose “No, change it” and say so.</p>}
              <Field label="Tool name">
                <input value={name} onChange={(e) => { nameEdited.current = true; setName(e.target.value); }} />
              </Field>
              <h3>Is this right?</h3>

              {!name.trim() && <p id="save-reason">Give the tool a name before saving.</p>}
              <div className="actions">
                <button className="primary" disabled={busy || !name.trim()} aria-describedby={!name.trim() ? "save-reason" : undefined} onClick={() => void act(async () => { const savedTool = await backend.saveTool(build.buildId, name); saveBotIdentity(savedTool.id, botSeed); await saved(); })}>{editTool ? "Yes, save changes" : "Yes, save this tool"}</button>
                <button onClick={changeRequest}>No, change it</button>
              </div>
            </>
          )}
          <details>
            <summary>Details</summary>
          {build.events.some(e=>/to review the output/i.test(e.message)) && !build.events.some(e=>/Checks failed/i.test(e.message)) ? <p role="status">The checks passed, so Rotebot had the model double-check the output against the page it read.</p> : build.events.some(e=>/correct|fix/i.test(e.message)) && <p role="status">{(build.events.some(e=>e.message.includes("(example format)")) || build.checks.some(c=>!c.passed && /example|shape|format/i.test(c.id))) ? "The output didn’t match your example’s format — Rotebot asked the model to fix it." : "The tool did not pass its checks — Rotebot asked the model to fix it."}</p>}
          {build.checks.some(c=>!c.passed && /example|shape|format/i.test(c.id)) && <p role="status">Tip: write values that change in {"{braces}"}, like {"{city}"} or {"{temp}"}.</p>}
            {build.failure && <p>{build.failure.message}</p>}
            {build.testModelCost && <ModelCostLines cost={build.testModelCost} />}
            {build.program && <SourceReview advanced={false} program={build.program} route={build.route} routeDetails={build.routeLabel} connections={draft?.requiredConnections} />}
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

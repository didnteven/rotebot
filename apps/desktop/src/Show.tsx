import { useEffect, useRef, useState } from "react";
import type { Backend, ShowState, RecordingFrames, RecordingPreview, TaskDraftV2, TaskDraft } from "./backend/types";
import { Notice, useAction, useScreenFocus } from "./ui";
import { CreateFlow } from "./CreateFlow";
import type { ComponentProps } from "react";

function ActivityLog({
  backend,
  state,
  update,
}: {
  backend: Backend;
  state: ShowState;
  update: (s: ShowState) => void;
}) {
  const { act, busy, error } = useAction();
  return (
    <section className="card">
      <h3>App activity log</h3>
      <Notice error={error} />
      {state.kind === "unavailable" ? (
        <p>{state.reason}</p>
      ) : state.kind === "recording" ? (
        <p>
          Recording {state.paused ? "paused" : "in progress"}. Use the timer bar
          to pause or stop.
        </p>
      ) : state.kind === "finished" ? (
        <>
          <h3>Recording summary</h3>
          <p>Duration: {state.durationSeconds} seconds</p>
          <p>Apps seen: {state.appsSeen.join(", ") || "No apps seen"}</p>
          <p>
            Connection suggestions appear above. Connecting grants no tool
            access.
          </p>
          <button
            onClick={() =>
              void act(async () => update(await backend.startShow()))
            }
            disabled={busy}
          >
            Record another task
          </button>
        </>
      ) : (
        <>
          <p>One task · maximum 5 minutes</p>
          <p>
            Records which apps you use and whether a window matches a known
            service like Gmail — never screen pixels, keystrokes, or window
            text.
          </p>
          <p>
            Excluded: password managers, finance apps, private browser windows,
            terminals. No screen access or Input Monitoring is requested.
          </p>
          <p>
            Accessibility: {state.accessibility}. Accessibility permits local
            app and service matching; raw window titles are discarded. Recording
            requires your explicit start.
          </p>
          {state.accessibility !== "granted" && (
            <p>
              Enable Accessibility for Macroscope in macOS System Settings, then
              check again. You can describe a task without recording.
            </p>
          )}
          <div className="actions">
            <button
              onClick={() =>
                void act(async () => update(await backend.showState()))
              }
            >
              Check Accessibility again
            </button>
            <button
              disabled={busy || state.accessibility !== "granted"}
              onClick={() =>
                void act(async () => update(await backend.startShow()))
              }
            >
              Start recording
            </button>
          </div>
        </>
      )}
    </section>
  );
}
export function RecordingBar({
  backend,
  state,
  update,
}: {
  backend: Backend;
  state: Extract<ShowState, { kind: "recording" }>;
  update: (s: ShowState) => void;
}) {
  const [now, setNow] = useState(Date.now());
  const { act, busy, error } = useAction();
  useEffect(() => {
    const timer = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(timer);
  }, []);
  const seconds = Math.min(
    state.maxSeconds,
    Math.max(0, Math.floor((now - state.startedAtMs) / 1000)),
  );
  return (
    <div className="recording-bar">
      <strong>{state.paused ? "Paused" : "Recording"}</strong>
      <span role="timer" aria-label="Recording timer">
        {Math.floor(seconds / 60)}:{String(seconds % 60).padStart(2, "0")} /
        5:00
      </span>
      <span className="sr-only" aria-live="polite">
        {state.paused
          ? "Recording paused"
          : `${Math.floor(seconds / 60)} minutes recorded`}
      </span>
      <span>{state.matches} service matches</span>
      <button
        disabled={busy}
        onClick={() =>
          void act(async () => update(await backend.pauseShow(!state.paused)))
        }
      >
        {state.paused ? "Resume" : "Pause"}
      </button>
      <button
        disabled={busy}
        onClick={() => void act(async () => update(await backend.stopShow()))}
      >
        Stop
      </button>
      <Notice error={error} />
    </div>
  );
}

export function ShowStatus({ state }: { state: ShowState | null }) {
  const [now, setNow] = useState(Date.now());
  const recording = state?.kind === "recording";
  useEffect(() => {
    if (!recording) return;
    setNow(Date.now());
    const timer = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(timer);
  }, [recording]);
  const seconds =
    state?.kind === "recording"
      ? Math.min(
          state.maxSeconds,
          Math.max(0, Math.floor((now - state.startedAtMs) / 1000)),
        )
      : 0;
  return (
    <span>
      Record a task:{" "}
      {state === null
        ? "Checking…"
        : state.kind === "recording"
          ? state.paused
            ? "Paused"
            : `Recording ${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, "0")}`
          : "Off"}
    </span>
  );
}


type CreateOptions = Omit<ComponentProps<typeof CreateFlow>, "backend" | "initialInput" | "initialGoal" | "initialDraft">;
export function ShowPanel({backend, state, update, close, createFlow}: {
  backend: Backend; state: ShowState; update: (s: ShowState) => void; close: () => void; createFlow: CreateOptions;
}) {
  const [step,setStep]=useState(0);
  const heading=useScreenFocus<HTMLHeadingElement>(step);
  const [available,setAvailable]=useState<boolean|null>(null);
  const [session,setSession]=useState<string|null>(null);
  const sessionRef=useRef<string|null>(null);
  const mounted=useRef(true);
  const [pollError,setPollError]=useState<string|null>(null);
  const [kept,setKept]=useState<RecordingFrames>({frames:[],dropped:[],stopped:false});
  const [preview,setPreview]=useState<RecordingPreview|null>(null);
  const [understood,setUnderstood]=useState<TaskDraftV2|null>(null);
  const [answers,setAnswers]=useState<Record<string,string>>({});
  const [draft,setDraft]=useState<TaskDraft|null>(null);
  const [started,setStarted]=useState(0),[elapsed,setElapsed]=useState(0);
  const {act,busy,error}=useAction();
  useEffect(()=>{ void act(async()=>setAvailable((await backend.recordingStatus()).captureAvailable)); },[backend]);
  useEffect(()=>{
    mounted.current=true;
    return ()=>{
      mounted.current=false;
      if(sessionRef.current) void backend.discardRecording(sessionRef.current).catch(()=>{});
    };
  },[backend]);
  useEffect(()=>{
    if(step!==1 || !session) return;
    let active=true;
    const timer=setInterval(()=>{
      setElapsed(Math.floor((Date.now()-started)/1000));
      void backend.listRecordingFrames(session).then(f=>{if(active)setKept(f);}).catch(e=>{
        if(active) {
          setPollError(e instanceof Error ? e.message : "Couldn’t check the recording. Stop or discard it and try again.");
          clearInterval(timer);
        }
      });
    },1000);
    return ()=>{active=false;clearInterval(timer);};
  },[step,session,started,backend]);
  async function discard(exit=false) {
    if(sessionRef.current) await backend.discardRecording(sessionRef.current);
    sessionRef.current=null; setSession(null); setPollError(null); setPreview(null); setUnderstood(null); setAnswers({});setDraft(null);setStep(0);
    if(exit)close(); else heading.current?.focus();
  }
  async function start(source:"sample"|"window") {
    const id=await backend.startRecording(source);
    if(!mounted.current) { await backend.discardRecording(id); return; }
    sessionRef.current=id;setSession(id);setPollError(null);
    setKept(await backend.listRecordingFrames(id));setStarted(Date.now());setElapsed(0);setStep(1);
  }
  const titles=["Show Macroscope what to do","Recording","Check what will be sent","What we understood","Create the tool"];
  return <section className="card recording-flow" aria-label="Record a task">
    {step<4 && <button disabled={busy} onClick={()=>void act(()=>discard(true))}>Back</button>}
    <h2 ref={heading} tabIndex={-1}>{titles[step]}</h2>
    <Notice error={error ?? pollError}/>
    {(error || pollError) && <p>Check your settings or discard this recording and start again.</p>}
    {busy && <p role="status">{step===2?"Analyzing your recording…":"Working…"}</p>}
    {step===0 && <>
      <p>Choose one window and show Macroscope a chore. You'll check the private details we've hidden, then decide whether to send it and turn it into a tool.</p>
      <p>Passwords, card numbers and private windows are excluded. If a frame can't be checked safely, it is left out.</p>
      <p className="muted">Privacy checks can miss sensitive details. Always check every image before sending.</p>
      {available===null ? <p role="status">Checking screen recording…</p> : available ?
        <button className="primary" disabled={busy} onClick={()=>void act(()=>start("window"))}>Choose a window to record</button> : <>
          <p className="notice notice-info">Screen recording isn't set up on this Mac yet</p>
          <p>The sample uses made-up information. You'll review it before deciding whether to send it.</p>
          <button className="primary" disabled={busy} onClick={()=>void act(()=>start("sample"))}>Try it with a sample recording</button>
        </>}
    </>}
    {step===1 && <>
      <p role="timer" aria-label="Elapsed recording time">{Math.floor(elapsed/60)}:{String(elapsed%60).padStart(2,"0")}</p>
      <p>Frames kept: {kept.frames.length}, hidden for privacy: {kept.dropped.length + kept.frames.filter(f=>f.masks.length>0).length}</p>
      {kept.stopped && <p role="status">The recording limit was reached. Stop to check what was kept.</p>}
      <button className="primary" disabled={busy} onClick={()=>void act(async()=>{
        await backend.stopRecording(session!);const p=await backend.previewRecording(session!);setPreview(p);setStep(2);
      })}>Stop</button>
    </>}
    {step===2 && preview && <>
      <p>Nothing leaves your Mac until you press Send.</p>
      {preview.sample && <p className="notice notice-info">Sample recording · made-up information. Check the recipient below before sending.</p>}
      <div className="recording-gallery" aria-label="Exact masked frames">
        {preview.frames.map(frame=><figure key={frame.seq}>
          <img src={`data:image/png;base64,${frame.pngBase64}`} width={frame.width} height={frame.height} alt={`Reviewed frame ${frame.seq}${frame.masks.length?", private content hidden in black":""}`}/>
          <figcaption>Frame {frame.seq} · {frame.masks.length ? `${frame.masks.length} area(s) hidden for privacy (black)` : "No areas hidden"}</figcaption>
        </figure>)}
      </div>
      {!preview.frames.length && <p>No frames were safe to keep. Discard this recording and try again.</p>}
      {preview.dropped.length>0 && <p>{preview.dropped.length} frame(s) left out for privacy.</p>}
      <h3>The text that goes with the images</h3><pre tabIndex={0} aria-label="Cleared recording text">{preview.clearedTextJson}</pre>
      <dl><dt>Recipient</dt><dd>{preview.recipient}</dd><dt>Model</dt><dd>{preview.model}</dd><dt>Size</dt><dd>{preview.totalBytes.toLocaleString()} bytes before delivery packaging</dd><dt>Estimated cost</dt><dd>${(preview.cost.estimated_micros/1_000_000).toFixed(4)}</dd></dl>
      <p>{preview.cost.caveat}</p>
      <details><summary>Instructions and response format</summary><h3>System instructions</h3><pre tabIndex={0}>{preview.systemInstructions}</pre><h3>Response schema</h3><pre tabIndex={0}>{preview.schemaJson}</pre><h3>Exact text fields</h3><pre tabIndex={0}>{preview.payloadText}</pre></details>
      <p>Pressing Send gives permission for this exact recording and model only.</p>
      <button className="primary" disabled={busy || !preview.frames.length} onClick={()=>void act(async()=>{
        const d=await backend.consentAndAnalyze(session!,preview.payloadHash);
        setUnderstood(d);setAnswers(Object.fromEntries(d.questions.map(q=>[q.id,q.default])));setPreview(null);setStep(3);
      })}>Send for analysis</button>
    </>}
    {step===3 && understood && <>
      <h3>What the tool can do</h3><ol>{understood.steps.filter(s=>s.capability!=="manual").map(s=><li key={s.id}>{s.action}</li>)}</ol>
      {understood.steps.some(s=>s.capability==="manual") && <><h3>You'll still need to do this part yourself</h3><ul>{understood.steps.filter(s=>s.capability==="manual").map(s=><li key={s.id}>{s.action}. {understood.unsupported_steps.find(u=>u.step_id===s.id)?.reason}</li>)}</ul></>}
      <h3>This changes each time</h3>{understood.variables.length?<ul>{understood.variables.map(v=><li key={v.name}>{v.name}: {v.description}</li>)}</ul>:<p>No changing details were found.</p>}
      {!!understood.constants.length && <><h3>What stays the same</h3><ul>{understood.constants.map(c=><li key={c.name}>{c.name}: {c.value}</li>)}</ul></>}
      <p>Result: {understood.output_description}</p><p>When: {understood.trigger}</p>
      {!!understood.ambiguities.length && <><h3>Still unclear</h3><ul>{understood.ambiguities.map((a,i)=><li key={i}>{a}</li>)}</ul></>}
      <p>Check each choice below. The suggested choices are already selected; “Any is fine” lets Macroscope choose within the plan.</p>
      {understood.questions.map(q=><fieldset key={q.id} disabled={busy}><legend>{q.prompt}</legend>{q.options.map(o=><label className="recording-choice" key={o.id}>
        <input type="radio" name={`recording-${q.id}`} value={o.id} checked={answers[q.id]===o.id} onChange={()=>setAnswers(a=>({...a,[q.id]:o.id}))}/>{o.any_is_fine?"Any is fine":o.label}{q.default===o.id?" (suggested)":""}
      </label>)}</fieldset>)}
      <button className="primary" disabled={busy || understood.questions.some(q=>!q.options.some(o=>o.id===answers[q.id]))} onClick={()=>void act(async()=>{
        await backend.answerRecordingQuestions(session!,answers);
        const approved=await backend.approveRecordingDraft(session!);sessionRef.current=null;setSession(null);setDraft(approved);setStep(4);
      })}>Looks right</button>
    </>}
    {step<4 && <button disabled={busy} onClick={()=>void act(()=>discard())}>Discard recording</button>}
    {step===4 && draft && <>
      <p>Your recording is discarded. Check the plan below, then build and save your tool.</p>
      <CreateFlow {...createFlow} backend={backend} initialInput={null} initialGoal={draft.goal} initialDraft={draft}/>
    </>}
    {step===0 && <details><summary>Advanced: app activity log</summary><ActivityLog backend={backend} state={state} update={update}/></details>}
  </section>;
}

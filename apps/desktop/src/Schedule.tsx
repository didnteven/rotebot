import { SourceReview } from "./SourceReview";
import { useEffect, useMemo, useRef, useState } from "react";
import { numberChoices, readNumber } from "./conditionChoices";
import type { Backend, ConnectorInfo, Frequency, Schedule, ScheduleOutputs, SendWhen, ToolDetail } from "./backend/types";
import { BotStatus } from "./Bot";
import { Notice, useAction } from "./ui";
const weekdays=["Monday","Tuesday","Wednesday","Thursday","Friday","Saturday","Sunday"];
// Connections that can send a message: an email account signed in with an app password, or Gmail by Google sign-in.
export const canSendEmail=(c:ConnectorInfo)=>c.state.kind==="connected" && (!!c.mail || c.mcpUrl.startsWith("https://gmail.googleapis.com/gmail/v1"));
export const validEmail=(to:string)=>/^[A-Za-z0-9._+-]+@[A-Za-z0-9-]+(\.[A-Za-z0-9-]+)+$/.test(to.trim());
export const whenText=(w:SendWhen)=>w.kind==="changed"?"the result changes":w.kind==="contains"?`the result contains “${w.text.trim()}”`:w.kind==="above"||w.kind==="below"?`${w.near.trim()?`the number ${w.side} “${w.near.trim()}”`:"the first number"} is ${w.kind} ${w.value}`:"";
export const whenProblem=(w:SendWhen)=>w.kind==="contains"&&!w.text.trim()?"Enter the text the result must contain.":(w.kind==="above"||w.kind==="below")&&!Number.isFinite(w.value)?"Enter the number to compare with.":null;
/** "Every hour at :15", "Twice a day at 8:00 and 18:00", "Every day at 7:00"... */
export function frequencyText(f: Frequency) {
  const t = (time: string) => time.replace(/^0(?=\d:)/, "");
  return f.kind === "hourly" ? `Every hour at :${f.time.slice(3)}`
    : f.kind === "twice_daily" ? `Twice a day at ${t(f.time)} and ${t(f.secondTime)}`
    : f.kind === "daily" ? `Every day at ${t(f.time)}`
    : f.kind === "weekly" ? `Every ${weekdays[f.weekday - 1]} at ${t(f.time)}`
    : `Every month on day ${f.day} (or the last day) at ${t(f.time)}`;
}
export function nextRunText(at: number) {
  const d=new Date(at), tomorrow=new Date(); tomorrow.setDate(tomorrow.getDate()+1);
  const day=d.toDateString()===tomorrow.toDateString()?"Tomorrow":d.toLocaleDateString(undefined,{weekday:"long",month:"short",day:"numeric"});
  return `Next run: ${day} ${d.getHours()}:${String(d.getMinutes()).padStart(2,"0")}`;
}
// "weather.txt" -> "weather-{date}.txt", so each scheduled run creates a new file.
export function datedFilename(outputName?: string) {
  const name=outputName?.trim() || "result.txt", dot=name.lastIndexOf(".");
  return dot>0 ? `${name.slice(0,dot)}-{date}${name.slice(dot)}` : `${name}-{date}`;
}
export function ScheduleSection({backend,tool,openSettings,advanced=true,onChanged}: {backend:Backend;tool:ToolDetail;openSettings?:()=>void;advanced?:boolean;onChanged?:()=>void}) {
  const [modelRoute,setModelRoute]=useState<import("./backend/types").ModelRoute>("offline_demo");
  const [routeDetails,setRouteDetails]=useState("");
  useEffect(()=>{void backend.modelAccess().then(a=>{setModelRoute(a.effectiveRoute??"offline_demo");setRouteDetails(`Custom endpoint · ${a.customEndpoint.host} · ${a.customEndpoint.model}`);});},[backend]);
  const [schedule,setSchedule]=useState<Schedule | null>(null),[login,setLogin]=useState(true);
  const [kind,setKind]=useState<Frequency["kind"]>("daily"),[time,setTime]=useState("07:00"),[secondTime,setSecondTime]=useState("18:00"),[weekday,setWeekday]=useState(1),[day,setDay]=useState(1);
  const [outputs,setOutputs]=useState<ScheduleOutputs>({today:true,notification:true,folder:null,filename:datedFilename(tool.program?.output_name)});
  const [file,setFile]=useState(false),[approved,setApproved]=useState(false);
  const [mailboxes,setMailboxes]=useState<ConnectorInfo[]>([]);
  // Connection names, so a saved API reads as "Akahu", never as its internal id.
  const [named,setNamed]=useState<import("./backend/types").SelectedConnection[]>([]);
  useEffect(()=>{let live=true;void backend.listConnectors().then(list=>{if(!live)return;setMailboxes(list.filter(canSendEmail));setNamed(list.map(c=>({connectorId:c.id,name:c.name,resource:c.mcpUrl,tools:[]})));}).catch(()=>{});return()=>{live=false;};},[backend]);
  // The tool's latest stored result: the schedule offers its numbers as choices instead of asking the person to type them.
  const [latest,setLatest]=useState(""),[manual,setManual]=useState(false);
  useEffect(()=>{let live=true;void backend.listToday().then(days=>{const e=days.flatMap(d=>d.entries).filter(x=>x.toolId===tool.id&&x.status==="done"&&x.body.trim()).sort((a,b)=>b.startedAtMs-a.startedAtMs)[0];if(live&&e)setLatest(e.body);}).catch(()=>{});return()=>{live=false;};},[backend,tool.id]);
  const choices=useMemo(()=>numberChoices(latest),[latest]);
  const emailOn=!!outputs.email;
  const when:SendWhen=outputs.sendWhen??{kind:"always"};
  const setWhen=(next:SendWhen)=>setOutputs({...outputs,sendWhen:next});
  const chooseWhen=(kind:string)=>setWhen(kind==="changed"?{kind}:kind==="contains"?{kind,text:""}:kind==="above"||kind==="below"?{kind,near:choices[0]?.near??"",side:choices[0]?.side??"after",value:Number.NaN}:{kind:"always"});
  const chooseMailbox=(id:string)=>{const box=mailboxes.find(c=>c.id===id);setOutputs({...outputs,email:{connectorId:id,to:box?.mail?.email??outputs.email?.to??""}});};
  const {act,error,busy}=useAction();
  const refresh=async()=>setSchedule((await backend.listSchedules()).find(s=>s.toolId===tool.id)??null);
  useEffect(()=>{void act(async()=>{await refresh();setLogin(await backend.loginEnabled());});},[backend,tool.id]);
  // The schedule just saved from this form already matches it, so loading it back must not clear the form or the approval.
  const skipLoad=useRef<string|null>(null),[savedKey,setSavedKey]=useState<string|null>(null);
  // Editing an existing schedule must start with its saved choices, not defaults.
  useEffect(() => {
    if (!schedule || skipLoad.current===schedule.id) return;
    setKind(schedule.frequency.kind);
    setTime(schedule.frequency.time);
    if (schedule.frequency.kind === "twice_daily") setSecondTime(schedule.frequency.secondTime);
    if (schedule.frequency.kind === "weekly") setWeekday(schedule.frequency.weekday);
    if (schedule.frequency.kind === "monthly") setDay(schedule.frequency.day);
    setOutputs(schedule.outputs);
    setFile(!!schedule.outputs.folder);
    setApproved(false);
  }, [schedule?.id]);
  // Approval is for exactly what the summary says, so it resets when that changes, not when the page merely re-renders.
  const approvedFor=JSON.stringify([kind,time,secondTime,weekday,day,outputs,file,tool.versionHash]);
  useEffect(()=>{setApproved(false);},[approvedFor]);
  const frequency:Frequency=kind==="hourly"?{kind,time:`00:${time.slice(3)}`}:kind==="twice_daily"?{kind,time,secondTime}:kind==="daily"?{kind,time}:kind==="weekly"?{kind,time,weekday}:{kind,time,day};
  const runsPerDay=kind==="hourly"?24:kind==="twice_daily"?2:1;
  const hosts=[...new Set(tool.program?.webSources.map(s=>{try{return new URL(s.url).hostname;}catch{return s.url;}})??[])];
  const actions=[...(hosts.length?[`read data from ${hosts.join(", ")}`]:[]),`run ${tool.name}`,...(outputs.today?["add the result to Today"]:[]),...(outputs.notification?["notify you"]:[]),...(outputs.email?[`email the result to ${outputs.email.to.trim()} from your ${mailboxes.find(c=>c.id===outputs.email?.connectorId)?.name??"email account"}`]:[]),...(file?[`create ${outputs.filename} in ${outputs.folder?.displayPath??"<folder>"}`]:[])];
  const validTime = (t: string) => /^([01]\d|2[0-3]):[0-5]\d$/.test(t);
  const invalidTime = !validTime(time) || (kind === "twice_daily" && (!validTime(secondTime) || secondTime === time));
  const invalidDay = kind === "monthly" && (!Number.isInteger(day) || day < 1 || day > 31);
  const scheduleProblem = invalidTime ? (kind === "twice_daily" ? "Choose two different times." : "Choose a valid time.") : invalidDay ? "Choose a day from 1 to 31."
    : file && (!outputs.folder || !outputs.filename.trim()) ? "Choose a folder and name for your result file."
    : outputs.email && !mailboxes.some(c=>c.id===outputs.email?.connectorId) ? "Choose which email account sends the result."
    : outputs.email && !validEmail(outputs.email.to) ? "Enter one email address, like you@example.com."
    : (outputs.notification||outputs.email) && whenProblem(when) ? whenProblem(when)
    : !outputs.today && !outputs.notification && !file && !outputs.email ? "Choose at least one place to receive your result."
    : !approved ? (schedule?"Approve the changes to update the schedule.":"Review the summary, then approve the schedule to continue.") : null;
  const saved=!!schedule && savedKey===approvedFor && schedule.status==="enabled";
  const approvedOutputs={...outputs,folder:file?outputs.folder:null};
  const tz=Intl.DateTimeFormat().resolvedOptions().timeZone;
  return <section className="card schedule" aria-label="Schedule"><h3>Schedule</h3>
    <p className="muted">Runs this tool automatically while Rotebot is open. Times are in {tz}.</p>{busy && <BotStatus label="Updating your schedule…" />}<Notice error={error}/>
    {schedule && <div className="actions schedule-state"><p role="status"><strong>{schedule.status==="needs_reapproval"?"Needs re-approval":schedule.status==="emergency_paused"?"Paused by emergency stop":schedule.status==="paused"?"Paused":schedule.nextRunMs?nextRunText(schedule.nextRunMs):"Enabled"}</strong></p>
      {schedule.status!=="needs_reapproval" && <button className="quiet" disabled={busy} onClick={()=>void act(async()=>{await backend.scheduleStatus(tool.id,schedule.status==="enabled"?"paused":"enabled");await refresh();onChanged?.();})}>{schedule.status==="enabled"?"Pause":"Resume"}</button>}
      <button className="quiet" disabled={busy} onClick={()=>void act(async()=>{await backend.scheduleStatus(tool.id,"remove");await refresh();setSavedKey(null);onChanged?.();})}>Remove</button></div>}
    <fieldset disabled={busy} className="schedule-form"><legend className="sr-only">When and where to run</legend>
      <div className="schedule-group"><h4>When</h4>
        <div className="schedule-row">
          <label>Frequency<select value={kind} onChange={e=>setKind(e.target.value as Frequency["kind"])}><option value="hourly">Every hour</option><option value="twice_daily">Twice a day</option><option value="daily">Daily</option><option value="weekly">Weekly</option><option value="monthly">Monthly</option></select></label>
          {kind==="weekly" && <label>Weekday<select value={weekday} onChange={e=>setWeekday(Number(e.target.value))}>{weekdays.map((name,i)=><option value={i+1} key={name}>{name}</option>)}</select></label>}
          {kind==="monthly" && <label>Day<input type="number" min={1} max={31} value={day} onChange={e=>setDay(Number(e.target.value))}/></label>}
          {kind==="hourly"
            ? <label>Minutes past the hour<input type="number" min={0} max={59} value={Number(time.slice(3))} onChange={e=>{const m=Math.min(59,Math.max(0,Math.trunc(Number(e.target.value)||0)));setTime(`00:${String(m).padStart(2,"0")}`);}}/></label>
            : <label>{kind==="twice_daily"?"First time":"Time"}<input type="time" value={time} onChange={e=>setTime(e.target.value)} required/></label>}
          {kind==="twice_daily" && <label>Second time<input type="time" value={secondTime} onChange={e=>setSecondTime(e.target.value)} required/></label>}
        </div>
        {runsPerDay>1 && tool.program?.modelStep && <p className="muted">Runs {runsPerDay} times a day. Each run uses the AI, which counts toward this tool’s monthly limit ({tool.modelStepCap} runs).</p>}
      </div>
      <div className="schedule-group"><h4>Where the result goes</h4>
        <label className="choice"><input type="checkbox" checked={outputs.today} onChange={e=>setOutputs({...outputs,today:e.target.checked})}/>Add to Today</label>
        <label className="choice"><input type="checkbox" checked={outputs.notification} onChange={e=>setOutputs({...outputs,notification:e.target.checked})}/>Notification</label>
        <label className="choice"><input type="checkbox" checked={emailOn} onChange={e=>{if(!e.target.checked)setOutputs({...outputs,email:null});else{const first=mailboxes[0];setOutputs({...outputs,email:{connectorId:first?.id??"",to:first?.mail?.email??""}});}}}/>Email me the result</label>
        {emailOn && <div className="schedule-sub">{mailboxes.length===0
          ? <p role="note" className="notice notice-info">Connect an email account in Connections first, then come back here. Your phone gets the email through its mail app.</p>
          : <>
              {mailboxes.length>1 && <label>Send from<select value={outputs.email?.connectorId} onChange={e=>chooseMailbox(e.target.value)}>{mailboxes.map(c=><option value={c.id} key={c.id}>{c.name}{c.mail?.email?` · ${c.mail.email}`:""}</option>)}</select></label>}
              <label>Send to<input type="email" value={outputs.email?.to??""} onChange={e=>setOutputs({...outputs,email:{connectorId:outputs.email?.connectorId??mailboxes[0].id,to:e.target.value}})} placeholder="you@example.com" autoComplete="off"/></label>
              <p className="muted">Sent from your own account to this one address.</p>
            </>}</div>}
        <label className="choice"><input type="checkbox" checked={file} onChange={e=>setFile(e.target.checked)}/>Create a file</label>
        {file && <div className="schedule-sub"><div className="actions"><button onClick={()=>void act(async()=>{const folder=await backend.pickOutputDir();if(folder)setOutputs({...outputs,folder});})}>Choose schedule folder</button>{outputs.folder && <span>{outputs.folder.displayPath}</span>}</div><label>File name template<input value={outputs.filename} onChange={e=>setOutputs({...outputs,filename:e.target.value})}/></label><p className="muted">{"{date}"} gives each day’s file its own name, for example {outputs.filename.replaceAll("{date}",new Date().toLocaleDateString("en-CA"))}.</p></div>}
      </div>
      {(outputs.notification||!!outputs.email) && <div className="schedule-group"><h4>Only send when</h4>
        <label>Send the {[outputs.notification?"notification":null,outputs.email?"email":null].filter(Boolean).join(" and ")}<select value={when.kind} onChange={e=>chooseWhen(e.target.value)}><option value="always">Every time it runs</option><option value="changed">Only when the result changes</option><option value="contains">Only when the result contains some text</option><option value="above">Only when a number is above a value</option><option value="below">Only when a number is below a value</option></select></label>
        {when.kind==="contains" && <label>Text<input value={when.text} onChange={e=>setWhen({kind:"contains",text:e.target.value})} placeholder="for example: earthquake"/></label>}
        {(when.kind==="above"||when.kind==="below") && <>
          {choices.length>0 && !manual && <label>Which number?<select value={String(choices.findIndex(c=>c.near===when.near&&c.side===when.side))} onChange={e=>{if(e.target.value==="-1")setManual(true);else{const c=choices[Number(e.target.value)];setWhen({...when,near:c.near,side:c.side});}}}>{choices.map((c,i)=><option value={i} key={`${c.side}${c.near}`}>{c.label}</option>)}<option value="-1">Another number…</option></select></label>}
          {choices.length>0 && !manual && Number.isFinite(readNumber(latest,when.near,when.side)??Number.NaN) && <p className="muted">Right now: {readNumber(latest,when.near,when.side)}. Pick a value to compare with.</p>}
          {(choices.length===0||manual) && <><label>Read the number<select value={when.side} onChange={e=>setWhen({...when,side:e.target.value as "after"|"before"})}><option value="after">right after the text</option><option value="before">right before the text</option></select></label>
          <label>Text next to the number<input value={when.near} onChange={e=>setWhen({...when,near:e.target.value})} placeholder={when.side==="after"?"for example: Tide":"for example: %"}/></label></>}
          <label>{when.kind==="above"?"Above":"Below"}<input type="number" step="any" value={Number.isFinite(when.value)?when.value:""} onChange={e=>setWhen({...when,value:e.target.value===""?Number.NaN:Number(e.target.value)})}/></label>
          <p className="muted">{choices.length===0?"Run the tool once and its numbers will be offered here to pick from. ":""}Leave the text empty to use the first number in the result. A unit such as % or km is the most dependable thing to read next to. If the number can’t be found, the result is sent anyway, with a note.</p>
        </>}
        {when.kind!=="always" && <p className="muted">Every run is still saved in Today, with the reason when nothing is sent.</p>}
      </div>}
      <div className="schedule-summary">
        <p data-testid="schedule-summary">{frequencyText(frequency)} Rotebot will {actions.slice(0,-1).join(", ")}{actions.length>1?", and ":""}{actions.at(-1)}.{when.kind!=="always"&&(outputs.notification||outputs.email)?` It will notify and email only when ${whenText(when)}.`:""}</p>
        {tool.program && <SourceReview advanced={advanced} program={tool.program} route={modelRoute} routeDetails={routeDetails} connections={named} frequency={frequencyText(frequency).replace(/^E/,"e").replace(/^T/,"t").replace(" (or the last day)","")}/>}
        <p className="muted">Any send or change it plans waits in Needs you for your approval. A run missed while Rotebot was closed happens once when it opens.</p>
      </div>
      {saved
        ? <p role="status" className="notice notice-info"><strong>Schedule is on.</strong> {schedule?.nextRunMs?nextRunText(schedule.nextRunMs)+". ":""}Change anything above to edit it; you will be asked to approve the change.</p>
        : <div className="schedule-approve">
            <label className="choice"><input type="checkbox" checked={approved} onChange={e=>setApproved(e.target.checked)}/>I approve this running unattended</label>
            {scheduleProblem && <p id={`schedule-reason-${tool.id}`} className="muted">{scheduleProblem}</p>}
            <button className="primary" aria-describedby={scheduleProblem ? `schedule-reason-${tool.id}` : undefined} disabled={busy || !!scheduleProblem} onClick={()=>void act(async()=>{const key=approvedFor,made=await backend.setSchedule(tool.id,{frequency,outputs:approvedOutputs,versionHash:tool.versionHash,approved});skipLoad.current=made.id;setSchedule(made);setSavedKey(key);setApproved(false);onChanged?.();})}>{schedule?"Update schedule":"Turn on schedule"}</button>
          </div>}
    </fieldset>
    {!login && <p>Schedules run while Rotebot is open. <button onClick={openSettings}>Open Settings to enable opening at login</button></p>}
  </section>;
}

import { SourceReview } from "./SourceReview";
import { useEffect, useState } from "react";
import type { Backend, Frequency, Schedule, ScheduleOutputs, ToolDetail } from "./backend/types";
import { Notice, useAction } from "./ui";
const weekdays=["Monday","Tuesday","Wednesday","Thursday","Friday","Saturday","Sunday"];
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
export function ScheduleSection({backend,tool,openSettings,advanced=true}: {backend:Backend;tool:ToolDetail;openSettings?:()=>void;advanced?:boolean}) {
  const [modelRoute,setModelRoute]=useState<import("./backend/types").ModelRoute>("offline_demo");
  const [routeDetails,setRouteDetails]=useState("");
  useEffect(()=>{void backend.modelAccess().then(a=>{setModelRoute(a.effectiveRoute??"offline_demo");setRouteDetails(`Custom endpoint · ${a.customEndpoint.host} · ${a.customEndpoint.model}`);});},[backend]);
  const [schedule,setSchedule]=useState<Schedule | null>(null),[login,setLogin]=useState(true);
  const [kind,setKind]=useState<Frequency["kind"]>("daily"),[time,setTime]=useState("07:00"),[weekday,setWeekday]=useState(1),[day,setDay]=useState(1);
  const [outputs,setOutputs]=useState<ScheduleOutputs>({today:true,notification:true,folder:null,filename:datedFilename(tool.program?.output_name)});
  const [file,setFile]=useState(false),[approved,setApproved]=useState(false);
  const {act,error,busy}=useAction();
  const refresh=async()=>setSchedule((await backend.listSchedules()).find(s=>s.toolId===tool.id)??null);
  useEffect(()=>{void act(async()=>{await refresh();setLogin(await backend.loginEnabled());});},[backend,tool.id]);
  useEffect(()=>{setApproved(false);},[kind,time,weekday,day,outputs,file,tool.versionHash]);
  const frequency:Frequency=kind==="daily"?{kind,time}:kind==="weekly"?{kind,time,weekday}:{kind,time,day};
  const hosts=[...new Set(tool.program?.webSources.map(s=>{try{return new URL(s.url).hostname;}catch{return s.url;}})??[])];
  const recurrence=kind==="daily"?"Every day":kind==="weekly"?`Every ${weekdays[weekday-1]}`:`Every month on day ${day} (or the last day)`;
  const actions=[...(hosts.length?[`read data from ${hosts.join(", ")}`]:[]),`run ${tool.name}`,...(outputs.today?["add the result to Today"]:[]),...(outputs.notification?["notify you"]:[]),...(file?[`create ${outputs.filename} in ${outputs.folder?.displayPath??"<folder>"}`]:[])];
  const approvedOutputs={...outputs,folder:file?outputs.folder:null};
  return <section className="card" aria-label="Schedule"><h3>Schedule</h3><Notice error={error}/>
    {schedule && <><p role="status">{schedule.status==="needs_reapproval"?"Needs re-approval":schedule.status==="emergency_paused"?"Paused by emergency stop":schedule.status==="paused"?"Paused":schedule.nextRunMs?nextRunText(schedule.nextRunMs):"Enabled"}</p>
      {schedule.status!=="needs_reapproval" && <button disabled={busy} onClick={()=>void act(async()=>{await backend.scheduleStatus(tool.id,schedule.status==="enabled"?"paused":"enabled");await refresh();})}>{schedule.status==="enabled"?"Pause":"Resume"}</button>}
      <button disabled={busy} onClick={()=>void act(async()=>{await backend.scheduleStatus(tool.id,"remove");await refresh();})}>Remove</button></>}
    <fieldset disabled={busy}><legend>Run unattended</legend>
      <label>Frequency<select value={kind} onChange={e=>setKind(e.target.value as Frequency["kind"])}><option value="daily">Daily</option><option value="weekly">Weekly</option><option value="monthly">Monthly</option></select></label>
      <label>Time<input type="time" value={time} onChange={e=>setTime(e.target.value)} required/></label>
      {kind==="weekly" && <label>Weekday<select value={weekday} onChange={e=>setWeekday(Number(e.target.value))}>{weekdays.map((name,i)=><option value={i+1} key={name}>{name}</option>)}</select></label>}
      {kind==="monthly" && <label>Day<input type="number" min={1} max={31} value={day} onChange={e=>setDay(Number(e.target.value))}/></label>}
      <label><input type="checkbox" checked={outputs.today} onChange={e=>setOutputs({...outputs,today:e.target.checked})}/>Add to Today</label>
      <label><input type="checkbox" checked={outputs.notification} onChange={e=>setOutputs({...outputs,notification:e.target.checked})}/>Notification</label>
      <label><input type="checkbox" checked={file} onChange={e=>setFile(e.target.checked)}/>Create a file</label>
      {file && <><button onClick={()=>void act(async()=>{const folder=await backend.pickOutputDir();if(folder)setOutputs({...outputs,folder});})}>Choose schedule folder</button><p>{outputs.folder?.displayPath}</p><label>File name template<input value={outputs.filename} onChange={e=>setOutputs({...outputs,filename:e.target.value})}/></label><p>Example: {outputs.filename.replaceAll("{date}",new Date().toLocaleDateString("en-CA"))}</p></>}
      <p>{recurrence} at {time.replace(/^0/,"")} Macroscope will {actions.slice(0,-1).join(", ")}{actions.length>1?", and ":""}{actions.at(-1)}.</p>
      <p>Connection actions wait in Needs you for approval of the exact calls. Macroscope runs schedules while open; missed runs happen once when it opens again.</p>
      {tool.program && <SourceReview advanced={advanced} program={tool.program} route={modelRoute} routeDetails={routeDetails} frequency={`${kind==="daily"?"every day":kind==="weekly"?`every ${weekdays[weekday-1]}`:`every month on day ${day}`} at ${time.replace(/^0/,"")}`}/>}
      <label><input type="checkbox" checked={approved} onChange={e=>setApproved(e.target.checked)}/>I approve this running unattended</label>
      {(!approved || !time || (file && (!outputs.folder || !outputs.filename)) || (!outputs.today && !outputs.notification && !file)) && <p>Choose when to run and where to put the result, then approve the schedule above.</p>}
      <button disabled={!approved || !time || (file && (!outputs.folder || !outputs.filename)) || (!outputs.today && !outputs.notification && !file)} onClick={()=>void act(async()=>{setSchedule(await backend.setSchedule(tool.id,{frequency,outputs:approvedOutputs,versionHash:tool.versionHash,approved}));setApproved(false);})}>Turn on schedule</button>
    </fieldset>
    {!login && <p>Schedules run while Macroscope is open. <button onClick={openSettings}>Open Settings to enable opening at login</button></p>}
  </section>;
}

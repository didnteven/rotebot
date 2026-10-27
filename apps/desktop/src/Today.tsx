import { useEffect, useState } from "react";
import type { Backend, TodayDay, TodayEntry } from "./backend/types";
import { Notice, useAction } from "./ui";

export function Today({backend, tools}: {backend: Backend; tools: () => void}) {
  const [days,setDays]=useState<TodayDay[] | null>(null);
  const [reviewed,setReviewed]=useState<Record<string,boolean>>({});
  const {act,error,busy}=useAction();
  const refresh=async()=>setDays(await backend.listToday());
  useEffect(()=>{ void act(refresh); const timer=setInterval(()=>void act(refresh),30_000);return()=>clearInterval(timer); },[backend]);
  const needs=days?.flatMap(d=>d.entries).filter(e=>e.status==="needs_you") ?? [];
  const show=(e: TodayEntry) => <article className="card" key={e.id}>
    <h3>{e.toolName}</h3><p>{e.status.replaceAll("_"," ")} · {new Date(e.startedAtMs).toLocaleTimeString([], {hour:"numeric", minute:"2-digit"})}</p>
    {e.note && <p>{e.note}</p>}<pre>{e.body}</pre>{e.fileCreated && <p>Created {e.fileCreated}</p>}
    {e.status==="needs_you" && e.staged ? <>
      {(e.staged.plannedCalls ?? []).map((a,i)=><div key={i}><h4>{a.service} · {a.call.toolName}</h4><pre>{JSON.stringify(a.call.args,null,2)}</pre><p>Server’s claims: readOnlyHint = {String(a.readOnlyHint)}; destructiveHint = {String(a.destructiveHint)}</p></div>)}
      {(e.staged.plannedCalls ?? []).some(a=>a.readOnlyHint!==true || a.destructiveHint===true) && <label><input type="checkbox" checked={!!reviewed[e.id]} onChange={v=>setReviewed({...reviewed,[e.id]:v.target.checked})}/>I reviewed these actions</label>}
      <button disabled={busy || ((e.staged.plannedCalls ?? []).some(a=>a.readOnlyHint!==true || a.destructiveHint===true) && !reviewed[e.id])} onClick={()=>void act(async()=>{const result=await backend.commitRun(e.staged!.runId,e.staged!.planHash,!!reviewed[e.id]);if("failure" in result) throw new Error(result.failure.message);await refresh();})}>Approve and run</button>
      <button disabled={busy} onClick={()=>void act(async()=>{await backend.discardRun(e.staged!.runId);await refresh();})}>Discard</button>
    </> : e.status!=="staged" && <button disabled={busy} onClick={()=>void act(async()=>{await backend.dismissTodayEntry(e.id);await refresh();})}>Dismiss</button>}
  </article>;
  return <section><Notice error={error}/>{days===null ? <p>Loading Today…</p> : !days.some(d=>d.entries.length) ? <div className="empty-state"><p>Schedule a tool to see its results here each day</p><button onClick={tools}>Go to Tools</button></div> : <>
    {!!needs.length && <section><h2>Needs you</h2>{needs.map(show)}</section>}
    {days.map(d=><section key={d.day}><h2>{d.day===new Date().toLocaleDateString("en-CA") ? "Today's results" : d.day}</h2>{d.entries.filter(e=>e.status!=="needs_you").map(show)}</section>)}
  </>}</section>;
}

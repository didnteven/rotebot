import { BotStatus } from "./Bot";
import {useEffect, useState} from "react";
import type {Backend, CustomEndpointStatus, ModelAccessStatus} from "./backend/types";
import {Field, Notice, useAction} from "./ui";

export function CustomEndpointCard({backend,status,update}:{backend:Backend;status:CustomEndpointStatus;update:(s:ModelAccessStatus)=>void}) {
  const [url,setUrl]=useState(status.baseUrl??""), [model,setModel]=useState(status.model??""), [name,setName]=useState(status.displayName??""), [cap,setCap]=useState(String(status.monthlyCallCap));
  const {act,error,busy}=useAction();
  useEffect(()=>{setUrl(status.baseUrl??"");setModel(status.model??"");setName(status.displayName??"");setCap(String(status.monthlyCallCap));},[status.baseUrl,status.model,status.displayName,status.monthlyCallCap]);
  let valid=false;
  try { const u=new URL(url); valid=u.protocol==="https:" && !u.username && !u.password && !url.includes("?") && !url.includes("#") && !url.split("://")[1]?.split("/")[0].includes("@") && !!model.trim() && model.length<=256; } catch { /* invalid URL */ }
  const optionsValid=/^\d+$/.test(cap) && Number(cap)<=4294967295;
  const saved=valid && new URL(url).href===status.baseUrl && model.trim()===status.model;
  const noPath=valid && new URL(url).pathname==="/";
  // Saving a changed URL clears the passing test, which would grey out the route until retested; so test straight away.
  const save=async(thenTest:boolean)=>{await backend.saveCustomEndpoint(url,model);update(await backend.setCustomEndpointOptions(name||null,Number(cap)));if(thenTest)update(await backend.testCustomEndpoint());};
  return <article className="card" aria-label="Custom endpoint settings">
    <h3>Custom endpoint (OpenAI-compatible)</h3>
    <p className="muted">Optional advanced setup. Use this only if your AI provider has given you a server address, model name, and access token. You can skip this when using Muse or a Meta API key.</p>
    <Notice error={error}/>
    <Field label="Endpoint base URL"><input type="url" value={url} placeholder="https://name.modal.direct/v1" onChange={e=>setUrl(e.target.value)}/></Field>
    <p className="muted">Paste the secure server address from your provider, usually ending in /v1. Leave out sign-in details and anything after a ? or #.</p>
    {noPath&&<p className="muted" role="note">This URL has no path. Most OpenAI-compatible servers, including Modal, need /v1 at the end.</p>}
    <Field label="Endpoint model"><input value={model} onChange={e=>setModel(e.target.value)}/></Field>
    <Field label="Endpoint display name"><input value={name} maxLength={100} onChange={e=>setName(e.target.value)}/></Field>
    <Field label="Monthly endpoint call cap"><input type="number" min="0" step="1" value={cap} onChange={e=>setCap(e.target.value)}/></Field>
    <p>{status.callsThisMonth} calls this month · {status.host??"No service configured"} · {status.model??"No model"}</p>
    <p>Token: {status.tokenPresent?"Set":"Not set"}. Entered in a native hidden-input dialog.</p>
    <p className="muted">Modal proxy-auth token: choose Set Modal token… and paste the Token ID, then the Token Secret from Modal's "Token created" screen. Macroscope sends them as one Authorization: Bearer ID.SECRET header.</p>
    <p role="status">Last endpoint test: {status.lastTest?`${status.lastTest.status === "ok" ? "OK" : status.lastTest.status} · ${status.lastTest.latencyMs} ms`:"Not tested"}</p>
    {busy && <BotStatus label="Updating model endpoint…" />}
    <div className="actions">
      <button disabled={busy||!valid||!optionsValid} onClick={()=>void act(()=>save(status.tokenPresent))}>Save endpoint</button>
      <button disabled={busy} onClick={()=>void act(async()=>update(await backend.saveCustomEndpointToken()))}>Set token…</button>
      <button disabled={busy} onClick={()=>void act(async()=>update(await backend.saveCustomEndpointModalToken()))}>Set Modal token…</button>
      <button disabled={busy||!valid||!optionsValid||!status.tokenPresent} onClick={()=>void act(async()=>{if(saved)update(await backend.testCustomEndpoint());else await save(true);})}>Test endpoint</button>
      <button disabled={busy||!saved||!status.tokenPresent||status.lastTest?.status!=="ok"} onClick={()=>void act(async()=>update(await backend.selectRoute("custom_endpoint")))}>Use this route</button>
      <button disabled={busy} onClick={()=>void act(async()=>update(await backend.removeCustomEndpoint()))}>Remove</button>
    </div>
  </article>;
}

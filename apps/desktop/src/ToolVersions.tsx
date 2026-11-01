import {useState} from "react";
import type {Backend, ToolVersionInfo} from "./backend/types";
import {Notice, useAction} from "./ui";

/** The saved versions of a tool. Editing a task adds a version; any older one can be made current again. */
export function ToolVersions({backend,toolId,name,changed}:{backend:Backend;toolId:string;name:string;changed:()=>Promise<void>}) {
  const [versions,setVersions]=useState<ToolVersionInfo[]|null>(null), [open,setOpen]=useState(false);
  const {act,error,busy}=useAction();
  const load=async()=>setVersions(await backend.listToolVersions(toolId));
  return <div>
    <button className="ghost" aria-expanded={open} onClick={()=>{const next=!open;setOpen(next);if(next)void act(load);}}>Version history</button>
    {open&&<>
      <p>Each saved edit keeps an earlier version here. Restoring a version makes it the one used for future runs. Scheduled tools may need your approval again.</p>
      <Notice error={error}/>
      {versions===null?<p role="status">Loading…</p>:<ul className="plain-list" aria-label={`Versions of ${name}`}>
        {versions.map(v=><li key={v.hash}>
          <p>Version {v.number}{v.active?" (current)":""} · {new Date(v.createdAtMs).toLocaleString()}{v.goal?` · ${v.goal.length>120?`${v.goal.slice(0,120)}…`:v.goal}`:""}</p>
          {!v.active&&<button className="quiet" disabled={busy} aria-label={`Restore version ${v.number} of ${name}`} onClick={()=>void act(async()=>{await backend.restoreToolVersion(toolId,v.hash);await load();await changed();})}>Restore this version</button>}
        </li>)}
      </ul>}
    </>}
  </div>;
}

import type { Program, ModelRoute, SelectedConnection } from "./backend/types";

function serviceName(id: string, connections?: SelectedConnection[]) {
  return connections?.find(c => c.connectorId === id)?.name ?? ({gmail:"Gmail","google-calendar":"Google Calendar"} as Record<string,string>)[id] ?? id;
}
export function SourceReview({program, route, frequency="on every run", build=false, connections}: {program:Program;route:ModelRoute;frequency?:string;build?:boolean;connections?:SelectedConnection[]}) {
  const reads=program.connectionSources??[];
  const labels=reads.map(s=>{
    const what=s.toolName.includes("event") ? (s.argsJson.includes("{today")?"today's events":"events") : s.toolName.includes("thread")?"matching email threads":s.name;
    return {source:s,service:serviceName(s.connectorId,connections),what};
  });
  const destination=route==="muse_plan"?"Muse (plan)":route==="api_key"?"Meta (API key)":"the offline demo model";
  return <section aria-label="Connection reads and model step">
    {labels.map(({source,service,what})=><div key={source.name}><p>Reads {what} from {service} ({source.toolName})</p><pre>{source.argsJson}</pre>{build&&<p>This build will read your {service} {what === "today's events" ? "events for today" : what} once to test the tool.</p>}</div>)}
    {program.modelStep&&<><p>{labels.length?labels.map(l=>`${l.what[0].toUpperCase()}${l.what.slice(1)} from ${l.service}`).join(" and "):"The program's selected input and source data"} are sent to {destination} {frequency} to {program.modelStep.purpose[0]?.toLowerCase()}{program.modelStep.purpose.slice(1)}.</p><p>Sends this instruction: &quot;{program.modelStep.prompt}&quot;</p><p>The program sends its prompt and derived JSON data (up to 64 KB per call). Up to 3 calls per run. {build?"This build will send this data once per requested model step to test the tool.":""}</p></>}
  </section>;
}

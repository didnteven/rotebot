import type { Program, ModelRoute, SelectedConnection } from "./backend/types";

function serviceName(id: string, connections?: SelectedConnection[]) {
  return connections?.find(c => c.connectorId === id)?.name ?? ({gmail:"Gmail","google-calendar":"Google Calendar"} as Record<string,string>)[id] ?? id;
}
export function SourceReview({program, route, routeDetails, frequency="on every run", build=false, connections, advanced=true}: {program:Program;route:ModelRoute;routeDetails?:string;frequency?:string;build?:boolean;connections?:SelectedConnection[];advanced?:boolean}) {
  const reads=program.connectionSources??[];
  const labels=reads.map(s=>{
    const what=s.toolName.includes("event") ? (s.argsJson.includes("{today")?"today's events":"events") : s.toolName.includes("thread")?"matching email threads":s.name;
    return {source:s,service:serviceName(s.connectorId,connections),what};
  });
  const destination=route==="muse_plan"?"Muse (plan)":route==="api_key"?"Meta (API key)":route==="custom_endpoint"?routeDetails || "Custom endpoint":"the offline demo model";
  return <section aria-label="Connection reads and model step">
    {labels.map(({source,service,what})=><div key={source.name}><p>Reads {what} from {service}</p>{build&&<p>This build will read your {service} {what === "today's events" ? "events for today" : what} once to test the tool.</p>}</div>)}
    {program.modelStep&&<><p>{labels.length?labels.map(l=>`${l.what[0].toUpperCase()}${l.what.slice(1)} from ${l.service}`).join(" and "):"The program's selected input and source data"} are sent to {destination} {frequency} to {program.modelStep.purpose[0]?.toLowerCase()}{program.modelStep.purpose.slice(1)}.</p><p>Sends this instruction: &quot;{program.modelStep.prompt}&quot;</p>{build && <p>This build will send this data once per requested model step to test the tool.</p>}<p>{route==="muse_plan"?"Model calls count against your Muse plan.":route==="api_key"?"Model calls are billed to your Meta API key.":route==="custom_endpoint"?"Your endpoint sets the price of model calls; Macroscope does not know the cost.":"The offline demo has no model charges."}</p></>}
    {advanced && (reads.length>0 || program.modelStep) && <details><summary>Advanced details</summary><SourceTechnicalDetails program={program}/></details>}
  </section>;
}

export function SourceTechnicalDetails({program}: {program:Program}) {
  return <>{program.connectionSources?.map(s=><div key={s.name}><p>{s.toolName}</p><pre>{s.argsJson}</pre></div>)}{program.modelStep&&<p>The program sends its prompt and derived JSON data (up to 64 KB per call). Up to 3 calls per run.</p>}</>;
}

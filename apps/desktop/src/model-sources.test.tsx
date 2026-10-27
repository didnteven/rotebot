import {fireEvent,render,screen,waitFor} from "@testing-library/react";
import {expect,it,vi} from "vitest";
import {SourceReview} from "./SourceReview";
import {RunFlow} from "./RunFlow";
import {ScheduleSection} from "./Schedule";
import {MockBackend} from "./backend/mock";
import type {Program} from "./backend/types";
const program:Program={summary:"Calendar summary",output_name:"summary.txt",code:"function run(){}",webSources:[],connectionSources:[{name:"events",connectorId:"google-calendar",toolName:"list_events",argsJson:'{"start":"{today_start}","end":"{today_end}"}'}],modelStep:{purpose:"Write the summary"}};
it("outbound review names each connection read, its exact template and the model destination and frequency",()=>{
  render(<SourceReview program={program} route="muse_plan" frequency="each morning" build/>);
  expect(screen.getByText("Reads today's events from Google Calendar (list_events)")).toBeVisible();
  expect(screen.getByText("Today's events from Google Calendar are sent to Muse (plan) each morning to write the summary.")).toBeVisible();
  expect(screen.getByText("This build will read your Google Calendar events for today once to test the tool.")).toBeVisible();
  expect(screen.getByText(program.connectionSources![0].argsJson)).toBeVisible();
});
it("tool detail shows the monthly cap and edits it through the backend",async()=>{
  const backend=new MockBackend();const tool=await backend.installSampleTool();
  tool.program=program;tool.noInput=true;
  vi.spyOn(backend,"getTool").mockResolvedValue(tool);
  const cap=vi.spyOn(backend,"setModelStepCap").mockImplementation(async(_id,cap)=>({...tool,modelStepCap:cap}));
  render(<RunFlow backend={backend} tool={tool} close={vi.fn()} changed={async()=>{}}/>);
  const field=await screen.findByRole("spinbutton",{name:"Monthly model-step cap"});expect(field).toHaveValue(60);
  fireEvent.change(field,{target:{value:"24"}});
  await waitFor(()=>expect(cap).toHaveBeenCalledWith(tool.id,24));
  await waitFor(()=>expect(field).toHaveValue(24));
});
it("schedule approval includes the read and selected model route",async()=>{
  const backend=new MockBackend();await backend.saveApiKey("test-key");const tool=await backend.installSampleTool();tool.program=program;tool.noInput=true;
  render(<ScheduleSection backend={backend} tool={tool}/>);
  expect(await screen.findByText("Reads today's events from Google Calendar (list_events)")).toBeVisible();
  expect(await screen.findByText("Today's events from Google Calendar are sent to Meta (API key) every day at 7:00 to write the summary.")).toBeVisible();
  expect(screen.getByRole("button",{name:"Turn on schedule"})).toBeDisabled();
});
it("mock calendar builds pause for exact read consent and persist the model declaration and cap",async()=>{
  const backend=new MockBackend();await backend.saveApiKey("test-key");
  await backend.setClientCredentials("google-calendar","fake-client",null);
  const start=await backend.beginConnect("google-calendar");await backend.completeConnect(start.pendingId);
  const draft=await backend.draftTaskNoInput("Summarize my calendar every morning");
  await backend.updateTask(draft.draftId,"program","summary.txt",null);
  await backend.selectTaskConnections(draft.draftId,["google-calendar"]);
  const initial=await backend.startBuild(draft.draftId,{outboundReviewed:true});
  await waitFor(async()=>expect((await backend.buildState(initial.buildId)).program?.connectionSources).toHaveLength(1),{timeout:5000});
  const candidate=await backend.buildState(initial.buildId);expect(candidate.phase).toBe("candidate");
  await expect(backend.approveBuildSources(initial.buildId,{...candidate.program!,code:"changed"})).rejects.toThrow("Candidate changed");
  await backend.approveBuildSources(initial.buildId,candidate.program!);
  await waitFor(async()=>expect((await backend.buildState(initial.buildId)).phase).toBe("ready"),{timeout:5000});
  const tool=await backend.saveTool(initial.buildId,"Calendar");expect(tool.modelStepCap).toBe(60);expect(tool.program?.modelStep).toEqual({purpose:"Write the summary"});
});

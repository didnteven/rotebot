import { render,screen,waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect,it,vi } from "vitest";
import { App } from "./App";
import { Today } from "./Today";
import { ScheduleSection, datedFilename } from "./Schedule";
import { MockBackend } from "./backend/mock";
import type { Schedule,TodayEntry,ToolDetail,StagedRun } from "./backend/types";

async function weather(backend:MockBackend):Promise<ToolDetail> {
  const sample=await backend.installSampleTool();
  return {...sample,name:"Weather summary",noInput:true,program:{summary:"Weather",output_name:"weather.txt",code:"function run(){}",webSources:[{name:"forecast",url:"https://api.open-meteo.com/forecast"}]}};
}
it("Today empty state links to Tools and is first in navigation",async()=>{
  const user=userEvent.setup();render(<App backend={new MockBackend()}/>);
  const nav=screen.getByRole("navigation");expect(nav.querySelector("button")).toHaveTextContent("Today");
  await user.click(screen.getByRole("button",{name:"Today"}));
  expect(await screen.findByText("Schedule a tool to see its results here each day")).toBeVisible();
  await user.click(screen.getByRole("button",{name:"Go to My tools"}));expect(screen.getByRole("heading",{name:"My tools",level:1})).toBeVisible();
});
function pending():TodayEntry {
  const staged:StagedRun={runId:"run",toolId:"weather",input:{handleId:"none",displayName:"No input",displayPath:""},outputDir:{handleId:"folder",displayName:"Reports",displayPath:"/Reports"},outputName:"weather.txt",planHash:"exact-hash",rowsIn:0,rowsOut:1,outputColumns:[],previewRows:[],destinationExists:false,createsOutput:true,plannedCalls:[{service:"Mail",resource:"https://mail.invalid/mcp",call:{connectorId:"mail",toolName:"send",args:{to:"owner@example.invalid",subject:"Your forecast",body:"Exact message"}},readOnlyHint:true,destructiveHint:true}]};
  return {id:"entry",toolId:"weather",toolName:"Weather summary",trigger:"schedule",scheduleId:"s",startedAtMs:Date.now(),finishedAtMs:null,status:"needs_you",body:"Summary",note:null,fileCreated:null,runId:"run",staged,notification:true,addToToday:true,dismissed:false};
}
it("Needs you reviews exact calls before approval and can discard",async()=>{
  const user=userEvent.setup(),backend=new MockBackend();let e=pending();
  vi.spyOn(backend,"listToday").mockImplementation(async()=>[{day:"2026-10-28",entries:[e]}]);
  const commit=vi.spyOn(backend,"commitRun").mockImplementation(async()=>{e={...e,status:"done",staged:null};return {runId:"run",toolId:"weather",toolName:"Weather",versionHash:"v",planHash:"exact-hash",inputDigest:"",committedDigest:"",outputPath:"/Reports/weather.txt",status:"committed",atMs:Date.now()};});
  const discard=vi.spyOn(backend,"discardRun").mockImplementation(async()=>{e={...e,status:"skipped",staged:null};});
  const view=render(<Today backend={backend} tools={()=>{}} create={()=>{}}/>);
  expect(await screen.findByRole("heading",{name:"Needs you"})).toBeVisible();expect(screen.getByText("owner@example.invalid",{exact:true})).toBeVisible();
  expect(screen.getByText("Your forecast")).toBeVisible();
  expect(screen.getByText("Exact message")).toBeVisible();
  expect(screen.getByText("This changes data in Mail")).toBeVisible();
  expect(screen.getByLabelText("Exact action arguments 1")).not.toBeVisible();
  await user.click(screen.getByText("Details"));
  expect(screen.getByLabelText("Exact action arguments 1")).toBeVisible();
  expect(screen.getByText(/Server’s claims: readOnlyHint = true; destructiveHint = true/)).toBeVisible();
  expect(screen.getByRole("button",{name:"Approve and run"})).toBeDisabled();await user.click(screen.getByLabelText("I reviewed these actions"));await user.click(screen.getByRole("button",{name:"Approve and run"}));
  expect(commit).toHaveBeenCalledWith("run","exact-hash",true);await waitFor(()=>expect(screen.queryByRole("heading",{name:"Needs you"})).not.toBeInTheDocument());
  view.unmount();e=pending();render(<Today backend={backend} tools={()=>{}} create={()=>{}}/>);await user.click(await screen.findByRole("button",{name:"Discard"}));expect(discard).toHaveBeenCalledWith("run");expect(commit).toHaveBeenCalledTimes(1);
});
it("Schedule requires explicit unattended approval and shows next run and reapproval",async()=>{
  const user=userEvent.setup(),backend=new MockBackend(),tool=await weather(backend);
  const tomorrow=new Date();tomorrow.setDate(tomorrow.getDate()+1);tomorrow.setHours(7,0,0,0);
  const schedule:Schedule={id:"s",toolId:tool.id,frequency:{kind:"daily",time:"07:00"},outputs:{today:true,notification:true,folder:null,filename:"weather-{date}.txt"},versionHash:tool.versionHash,binding:"b",folderIdentity:null,status:"enabled",approvedAtMs:Date.now(),lastDueMs:Date.now(),nextRunMs:+tomorrow};
  vi.spyOn(backend,"listSchedules").mockResolvedValue([]);const save=vi.spyOn(backend,"setSchedule").mockResolvedValue(schedule);
  const view=render(<ScheduleSection backend={backend} tool={tool}/>);
  const button=screen.getByRole("button",{name:"Turn on schedule"});expect(button).toBeDisabled();expect(screen.getByText(/Every day at 7:00 Macroscope will read data from api.open-meteo.com, run Weather summary/)).toBeVisible();
  await user.click(screen.getByLabelText("I approve this running unattended"));expect(button).toBeEnabled();await user.click(button);
  expect(save).toHaveBeenCalledWith(tool.id,expect.objectContaining({approved:true,versionHash:tool.versionHash,outputs:expect.objectContaining({today:true,notification:true})}));expect(await screen.findByText("Next run: Tomorrow 7:00")).toBeVisible();
  await user.click(screen.getByLabelText("I approve this running unattended"));await user.selectOptions(screen.getByLabelText("Frequency"),"monthly");expect(button).toBeDisabled();expect(screen.getByLabelText("Day")).toBeVisible();
  view.unmount();vi.mocked(backend.listSchedules).mockResolvedValue([{...schedule,status:"needs_reapproval"}]);render(<ScheduleSection backend={backend} tool={tool}/>);expect(await screen.findByText("Needs re-approval")).toBeVisible();expect(screen.queryByRole("button",{name:"Resume"})).not.toBeInTheDocument();
});
it("Schedule file name template comes from the tool's output name",()=>{
  expect(datedFilename("weather.txt")).toBe("weather-{date}.txt");
  expect(datedFilename("bills.summary.csv")).toBe("bills.summary-{date}.csv");
  expect(datedFilename("notes")).toBe("notes-{date}");
  expect(datedFilename(undefined)).toBe("result-{date}.txt");
});
it("Settings login toggle is opt-in and persists through the backend",async()=>{
  const backend=new MockBackend(),user=userEvent.setup();const set=vi.spyOn(backend,"setLoginEnabled");render(<App backend={backend}/>);await user.click(screen.getByRole("button",{name:"Settings"}));const toggle=await screen.findByLabelText("Open Macroscope at login so schedules run");expect(toggle).not.toBeChecked();await user.click(toggle);expect(set).toHaveBeenCalledWith(true);await waitFor(()=>expect(toggle).toBeChecked());
});

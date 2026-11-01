import { render,screen,waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { StrictMode } from "react";
import { expect,it,vi } from "vitest";
import { App } from "./App";
import { Today } from "./Today";
import { ScheduleSection, datedFilename } from "./Schedule";
import { MockBackend } from "./backend/mock";
import type { ConnectorInfo,Schedule,TodayEntry,ToolDetail,StagedRun } from "./backend/types";

async function weather(backend:MockBackend):Promise<ToolDetail> {
  const sample=await backend.installSampleTool();
  return {...sample,name:"Weather summary",noInput:true,program:{summary:"Weather",output_name:"weather.txt",code:"function run(){}",webSources:[{name:"forecast",url:"https://api.open-meteo.com/forecast"}]}};
}
it("Today empty state links to Tools and is first in navigation",async()=>{
  const user=userEvent.setup();render(<App backend={new MockBackend()}/>);
  const nav=screen.getByRole("navigation");expect(nav.querySelector("button")).toHaveTextContent("Today");
  await user.click(screen.getByRole("button",{name:"Today"}));
  expect(await screen.findByText("Scheduled results show up here")).toBeVisible();
  await user.click(screen.getByRole("button",{name:"Schedule a tool"}));expect(screen.getByRole("heading",{name:"My tools",level:1})).toBeVisible();
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
  expect(screen.queryByRole("checkbox")).not.toBeInTheDocument();await user.click(screen.getByRole("button",{name:"Approve and run"}));
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
  expect(screen.getByText(/Schedule is on\./)).toBeVisible();expect(screen.queryByText("Review the summary, then approve the schedule to continue.")).toBeNull();await user.selectOptions(screen.getByLabelText("Frequency"),"monthly");expect(screen.getByText("Approve the changes to update the schedule.")).toBeVisible();expect(screen.getByRole("button",{name:"Update schedule"})).toBeDisabled();expect(screen.getByLabelText("Day")).toBeVisible();
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

const mailbox=(over:Partial<ConnectorInfo>={}):ConnectorInfo=>({id:"mail-gmail",name:"Gmail (app password)",category:"mail",mcpUrl:"https://imap.gmail.com",registration:"api",verification:"unverified",clientSetup:null,redirectUri:null,docsUrl:null,state:{kind:"connected",sinceMs:1,toolCount:7},hasClientCredentials:false,mail:{help:"",customServer:false,imapHost:"imap.gmail.com",imapPort:993,smtpHost:"smtp.gmail.com",smtpPort:465,smtpStarttls:false,email:"me@example.com"},...over});
it("Schedule can email the result: the address is prefilled, shown in the summary and approved with the schedule",async()=>{
  const user=userEvent.setup(),backend=new MockBackend(),tool=await weather(backend);
  vi.spyOn(backend,"listConnectors").mockResolvedValue([mailbox()]);vi.spyOn(backend,"listSchedules").mockResolvedValue([]);
  const save=vi.spyOn(backend,"setSchedule").mockImplementation(async(_id,r)=>({id:"s",toolId:tool.id,frequency:r.frequency,outputs:r.outputs,versionHash:r.versionHash,binding:"b",folderIdentity:null,status:"enabled",approvedAtMs:1,lastDueMs:1,nextRunMs:null}));
  render(<ScheduleSection backend={backend} tool={tool}/>);
  await user.click(screen.getByLabelText("Email me the result"));
  const to=await screen.findByLabelText("Send to");expect(to).toHaveValue("me@example.com");
  expect(screen.getByText(/email the result to me@example.com from your Gmail \(app password\)/)).toBeVisible();
  await user.click(screen.getByLabelText("I approve this running unattended"));
  await user.click(screen.getByRole("button",{name:"Turn on schedule"}));
  expect(save).toHaveBeenCalledWith(tool.id,expect.objectContaining({approved:true,outputs:expect.objectContaining({email:{connectorId:"mail-gmail",to:"me@example.com"}})}));
  expect(await screen.findByText(/Schedule is on\./)).toBeVisible();
  // Changing the address withdraws the approval, and only one plain address is accepted.
  await user.clear(to);await user.type(to,"a@b.com, c@d.com");
  expect(screen.getByRole("button",{name:"Update schedule"})).toBeDisabled();expect(screen.getByText("Enter one email address, like you@example.com.")).toBeVisible();
});
it("Email me the result explains what to do when no mailbox is connected, and never saves a blank address",async()=>{
  const user=userEvent.setup(),backend=new MockBackend(),tool=await weather(backend);
  vi.spyOn(backend,"listConnectors").mockResolvedValue([mailbox({state:{kind:"not_connected"}})]);vi.spyOn(backend,"listSchedules").mockResolvedValue([]);
  render(<ScheduleSection backend={backend} tool={tool}/>);
  await user.click(screen.getByLabelText("Email me the result"));
  expect(await screen.findByText(/Connect an email account in Connections first/)).toBeVisible();
  await user.click(screen.getByLabelText("I approve this running unattended"));
  expect(screen.getByRole("button",{name:"Turn on schedule"})).toBeDisabled();
});
it("a tool card shows its schedule and a Schedule button that opens the schedule section",async()=>{
  const user=userEvent.setup(),backend=new MockBackend(),tool=await weather(backend);
  const summary={id:tool.id,name:"Tide",version:1,engine:"js-quickjs" as const,createdAtMs:1,lastRunAtMs:null,runCount:0,sample:false,state:"enabled" as const};
  vi.spyOn(backend,"listTools").mockResolvedValue([summary] as never);vi.spyOn(backend,"getTool").mockResolvedValue(tool);vi.spyOn(backend,"listConnectors").mockResolvedValue([mailbox()]);
  const next=new Date();next.setDate(next.getDate()+1);next.setHours(7,0,0,0);
  const schedules:Schedule[]=[];vi.spyOn(backend,"listSchedules").mockImplementation(async()=>schedules);
  const first=render(<App backend={backend}/>);
  await user.click(await screen.findByRole("button",{name:"My tools"}));
  expect(await screen.findByRole("button",{name:"Schedule: Tide"})).toBeVisible();expect(screen.queryByTestId("schedule-line")).toBeNull();
  await user.click(screen.getByRole("button",{name:"Schedule: Tide"}));
  const summaryLine=await screen.findByText("Schedule this tool");expect(summaryLine.closest("details")).toHaveProperty("open",true);
  first.unmount();
  schedules.push({id:"s",toolId:tool.id,frequency:{kind:"daily",time:"07:00"},outputs:{today:true,notification:true,folder:null,filename:"x.txt",email:{connectorId:"mail-gmail",to:"me@example.com"}},versionHash:tool.versionHash,binding:"b",folderIdentity:null,status:"enabled",approvedAtMs:1,lastDueMs:1,nextRunMs:next.getTime()});
  render(<App backend={backend}/>);
  await user.click(screen.getByRole("button",{name:"My tools"}));
  expect(await screen.findByTestId("schedule-line")).toHaveTextContent("Daily · Next tomorrow 7:00 · Email + notify");
  expect(screen.getByRole("button",{name:"Schedule: Tide"})).toHaveTextContent("Edit schedule");
});

it("Send only when: choosing a condition asks for what it needs, shows it in the summary and approves it with the schedule",async()=>{
  const user=userEvent.setup(),backend=new MockBackend(),tool=await weather(backend);
  vi.spyOn(backend,"listConnectors").mockResolvedValue([mailbox()]);vi.spyOn(backend,"listSchedules").mockResolvedValue([]);
  const save=vi.spyOn(backend,"setSchedule").mockImplementation(async(_id,r)=>({id:"s",toolId:tool.id,frequency:r.frequency,outputs:r.outputs,versionHash:r.versionHash,binding:"b",folderIdentity:null,status:"enabled",approvedAtMs:1,lastDueMs:1,nextRunMs:null}));
  render(<ScheduleSection backend={backend} tool={tool}/>);
  // The notification is on by default, so the choice is offered straight away.
  const choice=screen.getByLabelText("Send the notification");await waitFor(()=>expect(choice).toBeEnabled());expect(choice).toHaveValue("always");
  await user.selectOptions(choice,"above");
  const approve=screen.getByLabelText("I approve this running unattended");await user.click(approve);
  expect(screen.getByRole("button",{name:"Turn on schedule"})).toBeDisabled();expect(screen.getByText("Enter the number to compare with.")).toBeVisible();
  await user.type(screen.getByLabelText("Text next to the number"),"Tide");await user.type(screen.getByLabelText("Above"),"80");
  expect(screen.getByText(/It will notify and email only when the number after “Tide” is above 80\./)).toBeVisible();
  await user.click(approve);await user.click(screen.getByRole("button",{name:"Turn on schedule"}));
  expect(save).toHaveBeenCalledWith(tool.id,expect.objectContaining({outputs:expect.objectContaining({sendWhen:{kind:"above",near:"Tide",side:"after",value:80}})}));
  // Reading the number before a unit.
  await user.selectOptions(screen.getByLabelText("Read the number"),"before");await user.clear(screen.getByLabelText("Text next to the number"));await user.type(screen.getByLabelText("Text next to the number"),"%");
  expect(screen.getByText(/the number before “%” is above 80/)).toBeVisible();
  // Contains needs text; a condition is hidden when nothing would be sent.
  await user.selectOptions(screen.getByLabelText("Send the notification"),"contains");expect(screen.getByText("Enter the text the result must contain.")).toBeVisible();
  await user.click(screen.getByLabelText("Notification"));expect(screen.queryByLabelText(/^Send the/)).toBeNull();
});
it("Send only when: the numbers come from the tool's stored result, so nothing has to be typed to find them",async()=>{
  const user=userEvent.setup(),backend=new MockBackend(),tool=await weather(backend);
  vi.spyOn(backend,"listConnectors").mockResolvedValue([]);vi.spyOn(backend,"listSchedules").mockResolvedValue([]);
  const entry={...pending(),toolId:tool.id,status:"done" as const,staged:null,body:"Mission Bay tide right now (2026-10-31T18:15): -0.98 m, Tide 9% (full being 100%)\n"};
  vi.spyOn(backend,"listToday").mockResolvedValue([{day:"2026-10-31",entries:[entry]}]);
  const save=vi.spyOn(backend,"setSchedule").mockImplementation(async(_id,r)=>({id:"s",toolId:tool.id,frequency:r.frequency,outputs:r.outputs,versionHash:r.versionHash,binding:"b",folderIdentity:null,status:"enabled",approvedAtMs:1,lastDueMs:1,nextRunMs:null}));
  render(<ScheduleSection backend={backend} tool={tool}/>);
  const when=screen.getByLabelText("Send the notification");await waitFor(()=>expect(when).toBeEnabled());
  await user.selectOptions(when,"below");
  const which=await screen.findByLabelText("Which number?");
  expect(screen.queryByLabelText("Text next to the number")).toBeNull();
  expect(screen.getByRole("option",{name:"Tide: 9"})).toBeVisible();expect(screen.getByRole("option",{name:"-0.98 m"})).toBeVisible();
  await user.selectOptions(which,screen.getByRole("option",{name:"Tide: 9"}));
  expect(screen.getByText(/Right now: 9\./)).toBeVisible();
  await user.type(screen.getByLabelText("Below"),"20");
  await user.click(screen.getByLabelText("I approve this running unattended"));
  await user.click(screen.getByRole("button",{name:"Turn on schedule"}));
  expect(save).toHaveBeenCalledWith(tool.id,expect.objectContaining({outputs:expect.objectContaining({sendWhen:{kind:"below",near:"Tide",side:"after",value:20}})}));
});
it("Approval stays ticked while the page re-renders and resets only when the schedule changes",async()=>{
  const user=userEvent.setup(),backend=new MockBackend(),tool=await weather(backend);
  vi.spyOn(backend,"listConnectors").mockResolvedValue([]);vi.spyOn(backend,"listSchedules").mockResolvedValue([]);
  const view=render(<ScheduleSection backend={backend} tool={tool}/>);
  const approve=screen.getByLabelText("I approve this running unattended");await waitFor(()=>expect(approve).toBeEnabled());
  await user.click(approve);
  view.rerender(<ScheduleSection backend={backend} tool={{...tool}}/>);
  expect(approve).toBeChecked();expect(screen.getByRole("button",{name:"Turn on schedule"})).toBeEnabled();
  await user.selectOptions(screen.getByLabelText("Frequency"),"weekly");
  expect(approve).not.toBeChecked();
});
it("from a tool card, a ticked approval stays ticked and turns the schedule button on (new and existing schedule)",async()=>{
  for (const existing of [false,true]) {
    const user=userEvent.setup(),backend=new MockBackend(),tool=await weather(backend);
    const summary={id:tool.id,name:"Tide",version:1,engine:"js-quickjs" as const,createdAtMs:1,lastRunAtMs:null,runCount:0,sample:false,state:"enabled" as const};
    vi.spyOn(backend,"listTools").mockResolvedValue([summary] as never);vi.spyOn(backend,"getTool").mockImplementation(async()=>({...tool}));vi.spyOn(backend,"listConnectors").mockResolvedValue([mailbox()]);
    const schedules:Schedule[]=existing?[{id:"s",toolId:tool.id,frequency:{kind:"daily",time:"07:00"},outputs:{today:true,notification:true,folder:null,filename:"x.txt"},versionHash:tool.versionHash,binding:"b",folderIdentity:null,status:"enabled",approvedAtMs:1,lastDueMs:1,nextRunMs:Date.now()+1000}]:[];
    vi.spyOn(backend,"listSchedules").mockImplementation(async()=>schedules.map(s=>({...s,outputs:{...s.outputs}})));
    const view=render(<StrictMode><App backend={backend}/></StrictMode>);
    await user.click(await screen.findByRole("button",{name:"My tools"}));
    await user.click(await screen.findByRole("button",{name:"Schedule: Tide"}));
    const approve=await screen.findByLabelText("I approve this running unattended");
    await waitFor(()=>expect(approve).toBeEnabled());await new Promise(r=>setTimeout(r,100));
    await user.click(approve);
    await new Promise(r=>setTimeout(r,300));
    expect(screen.getByLabelText("I approve this running unattended")).toBeChecked();
    expect(screen.getByRole("button",{name:existing?"Update schedule":"Turn on schedule"})).toBeEnabled();
    view.unmount();
  }
});
it("after turning a schedule on, the tool card shows it straight away",async()=>{
  const user=userEvent.setup(),backend=new MockBackend(),tool=await weather(backend);
  const summary={id:tool.id,name:"Tide",version:1,engine:"js-quickjs" as const,createdAtMs:1,lastRunAtMs:null,runCount:0,sample:false,state:"enabled" as const};
  vi.spyOn(backend,"listTools").mockResolvedValue([summary] as never);vi.spyOn(backend,"getTool").mockResolvedValue(tool);vi.spyOn(backend,"listConnectors").mockResolvedValue([]);
  const next=new Date();next.setDate(next.getDate()+1);next.setHours(7,0,0,0);
  const schedules:Schedule[]=[];vi.spyOn(backend,"listSchedules").mockImplementation(async()=>[...schedules]);
  vi.spyOn(backend,"setSchedule").mockImplementation(async(_id,r)=>{const s:Schedule={id:"s",toolId:tool.id,frequency:r.frequency,outputs:r.outputs,versionHash:r.versionHash,binding:"b",folderIdentity:null,status:"enabled",approvedAtMs:1,lastDueMs:1,nextRunMs:next.getTime()};schedules.push(s);return s;});
  render(<App backend={backend}/>);
  await user.click(await screen.findByRole("button",{name:"My tools"}));
  await user.click(await screen.findByRole("button",{name:"Schedule: Tide"}));
  const approve=await screen.findByLabelText("I approve this running unattended");await waitFor(()=>expect(approve).toBeEnabled());
  await user.click(approve);await user.click(screen.getByRole("button",{name:"Turn on schedule"}));
  expect(await screen.findByText(/Schedule is on\./)).toBeVisible();
  await user.click(screen.getByRole("button",{name:"Back"}));
  expect(await screen.findByTestId("schedule-line")).toHaveTextContent("Daily");
  expect(screen.getByRole("button",{name:"Schedule: Tide"})).toHaveTextContent("Edit schedule");
});

it("schedules a tool every hour or twice a day",async()=>{
  const user=userEvent.setup(),backend=new MockBackend(),tool=await weather(backend);
  vi.spyOn(backend,"getTool").mockResolvedValue(tool);
  const save=vi.spyOn(backend,"setSchedule");
  render(<ScheduleSection backend={backend} tool={tool}/>);
  const frequency=await screen.findByRole("combobox",{name:"Frequency"});
  await user.selectOptions(frequency,"twice_daily");
  await user.clear(screen.getByLabelText("First time"));await user.type(screen.getByLabelText("First time"),"08:00");
  await user.clear(screen.getByLabelText("Second time"));await user.type(screen.getByLabelText("Second time"),"08:00");
  expect(screen.getByText("Choose two different times.")).toBeVisible();
  await user.clear(screen.getByLabelText("Second time"));await user.type(screen.getByLabelText("Second time"),"18:30");
  expect(screen.getByTestId("schedule-summary")).toHaveTextContent(/^Twice a day at 8:00 and 18:30 Macroscope will/);
  await user.selectOptions(frequency,"hourly");
  const minutes=screen.getByLabelText("Minutes past the hour");await user.clear(minutes);await user.type(minutes,"5");
  expect(screen.getByTestId("schedule-summary")).toHaveTextContent(/^Every hour at :05 Macroscope will/);
  await user.click(screen.getByLabelText("I approve this running unattended"));
  await user.click(screen.getByRole("button",{name:"Turn on schedule"}));
  await waitFor(()=>expect(save).toHaveBeenCalledWith(tool.id,expect.objectContaining({frequency:{kind:"hourly",time:"00:05"}})));
  expect(await screen.findByText(/Schedule is on\./)).toBeVisible();
});

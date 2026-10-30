import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { expect, it, vi } from "vitest";
import { RunFlow } from "./RunFlow";
import { CreateFlow } from "./CreateFlow";
import { Connections } from "./Connections";
import { ReceiptCard } from "./ui";
import { MockBackend } from "./backend/mock";
import type { PlannedAction, Receipt, StagedRun } from "./backend/types";

const action: PlannedAction = {
  call: {connectorId:"gmail", toolName:"create_draft", args:{to:"test@example.invalid",subject:"Review this message",body:'<script>alert("args")</script><img src=x onerror=alert(1)>'}},
  service:"Gmail",resource:"https://example.invalid/mcp",readOnlyHint:false,destructiveHint:false,
};
const receipt: Receipt = {runId:"r",toolId:"t",toolName:"Draft message",versionHash:"v",planHash:"hash",inputDigest:"input",committedDigest:"",outputPath:"",status:"committed",atMs:1,
  actions:[{action,status:"succeeded",summary:'<script>alert("result")</script><iframe src="https://evil.invalid">'}],
};

it("renders complete action arguments and server claims as text and requires action review", async () => {
  const backend = new MockBackend();
  const tool = await backend.installSampleTool();
  const input = await backend.loadSampleCsv();
  const outputDir = (await backend.pickOutputDir())!;
  const staged: StagedRun = {createsOutput:false,runId:"r",toolId:tool.id,input:input.handle,outputDir,outputName:"draft.txt",planHash:"hash",rowsIn:1,rowsOut:0,outputColumns:[],previewRows:[],destinationExists:false,plannedCalls:[action]};
  vi.spyOn(backend, "stageRun").mockResolvedValue(staged);
  const commit = vi.spyOn(backend, "commitRun").mockResolvedValue(receipt);
  const user = userEvent.setup();
  const {container} = render(<RunFlow backend={backend} tool={tool} close={vi.fn()} changed={async()=>{}} />);
  await user.click(screen.getByRole("button", {name:"Use sample input"}));
  await user.click(screen.getByRole("button", {name:"Choose output folder"}));
  await user.click(screen.getByRole("button", {name:"Preview result"}));
  expect(await screen.findByText("test@example.invalid")).toBeVisible();
  expect(screen.getByText("Review this message")).toBeVisible();
  expect(screen.getByText("This changes data in Gmail")).toBeVisible();
  expect(screen.getByLabelText("Exact action arguments 1")).not.toBeVisible();
  expect(screen.getByText(/Server’s claims: readOnlyHint = false; destructiveHint = false/)).not.toBeVisible();
  await user.click(screen.getByText("Details"));
  expect(screen.getByText(/Server’s claims: readOnlyHint = false; destructiveHint = false/)).toBeVisible();
  expect(screen.getByLabelText("Exact action arguments 1").textContent).toBe(JSON.stringify(action.call.args, null, 2));
  const approve = screen.getByRole("button", {name:"Approve actions"});
  expect(approve).toBeDisabled();
  expect(commit).not.toHaveBeenCalled();
  await user.click(screen.getByRole("checkbox", {name:"I reviewed these actions"}));
  await user.click(approve);
  expect(commit).toHaveBeenCalledExactlyOnceWith("r", "hash", true);
  expect(await screen.findByText(receipt.actions![0].summary)).toBeVisible();
  expect(container.querySelectorAll("script,img,iframe,object")).toHaveLength(0);
  await user.click(screen.getByRole("button", {name:"Run again"}));
  await user.click(screen.getByRole("button", {name:"Preview result"}));
  expect(await screen.findByRole("checkbox", {name:"I reviewed these actions"})).not.toBeChecked();
  expect(screen.getByRole("button", {name:"Approve actions"})).toBeDisabled();
});

it("shows action receipts as inert text and disables Finder for action-only runs", () => {
  const {container} = render(<ReceiptCard backend={new MockBackend()} receipt={receipt} />);
  expect(screen.getByText(receipt.actions![0].summary)).toBeVisible();
  expect(screen.getByRole("button", {name:"Show in Finder"})).toBeDisabled();
  expect(container.querySelectorAll("script,img,iframe,object")).toHaveLength(0);
});

it("offers explicit access to connected services in Create and lists their untrusted tools", async () => {
  const backend = new MockBackend();
  const input = backend.importText("message.txt", "hello");
  const connector = (await backend.listConnectors())[0];
  connector.state = {kind:"connected",sinceMs:1,toolCount:1};
  vi.spyOn(backend,"listConnectors").mockResolvedValue([connector]);
  vi.spyOn(backend,"previewConnectorTools").mockResolvedValue([{name:"create_draft",description:"<script>server instructions</script>",inputSchema:{type:"object"},readOnly:false,destructive:false}]);
  const select = vi.spyOn(backend, "selectTaskConnections");
  const user = userEvent.setup();
  const {container} = render(<CreateFlow backend={backend} initialInput={input} status={await backend.modelAccess()} connectors={[connector]} openSettings={vi.fn()} connect={vi.fn()} close={vi.fn()} saved={async()=>{}} />);
  await user.type(screen.getByLabelText("What should the tool do?"), "draft a Gmail message");
  await user.click(screen.getByRole("button", {name:"Check the plan"}));
  await user.click(screen.getByText("Details"));
  const use = await screen.findByRole("checkbox", {name:`Let this tool use ${connector.name}`});
  // The description mentions Gmail and it is connected, so it is switched on for the tool already.
  expect(await screen.findByText(/Using your .* connection \(read-only\)/)).toBeVisible();
  expect(use).toBeChecked();
  expect(await screen.findByText("create_draft")).toBeVisible();
  expect(select).toHaveBeenCalledWith(expect.any(String), [connector.id]);
  expect(screen.getByText(/<script>server instructions<\/script>/)).toBeVisible();
  expect(container.querySelectorAll("script,img,iframe")).toHaveLength(0);
});

it("keeps every external field visible in plain words and hides only the exact argument object", async()=>{
  const {ActionReview, ActionTechnicalDetails} = await import("./ActionReview");
  const change:PlannedAction = {...action,service:"Calendar",readOnlyHint:null,destructiveHint:null,call:{...action.call,toolName:"create_event",args:{title:"Team catchup",location:"Office",start:"09:00",end:"09:30",notes:"Bring report",attendees:["owner@example.invalid"],options:{notify:true}}}};
  render(<><ActionReview action={change}/><details><summary>Advanced details</summary><ActionTechnicalDetails actions={[change]}/></details></>);
  expect(screen.getByText("Calendar · Create event")).toBeVisible();
  expect(screen.getByText("This changes data in Calendar")).toBeVisible();
  for(const value of ["Team catchup","Office","09:00","09:30","Bring report","owner@example.invalid","true"]) expect(screen.getByText(value,{exact:true})).toBeVisible();
  expect(screen.getByLabelText("Exact action arguments 1")).not.toBeVisible();
  await userEvent.setup().click(screen.getByText("Advanced details"));
  expect(screen.getByLabelText("Exact action arguments 1")).toHaveTextContent(JSON.stringify(change.call.args,null,2).replace(/\s+/g," "));
});

it("explains why Gmail cannot connect yet and moves focus to its details", async () => {
  const backend = new MockBackend();
  const connectors = await backend.listConnectors();
  const user = userEvent.setup();
  const Harness = () => {
    const [selected, setSelected] = useState<string | null>(null);
    return <Connections backend={backend} connectors={connectors} selected={selected} select={setSelected} changed={async () => {}} />;
  };
  render(<Harness />);
  await user.click(screen.getByRole("button", { name: "Connect Gmail" }));
  expect(await screen.findByText(/needs your own OAuth client before it can connect/)).toBeVisible();
  expect(screen.getByRole("heading", { name: "Gmail connection" })).toHaveFocus();
  expect(screen.getByText("Connect turns on after you save a client ID above.")).toBeVisible();
  expect(screen.getAllByRole("button", { name: "Connect" })[0]).toBeDisabled();
});

it("imports Google's client file and saves it for Gmail, Calendar and Drive", async () => {
  const backend = new MockBackend();
  const connectors = await backend.listConnectors();
  const save = vi.spyOn(backend, "setClientCredentials");
  const user = userEvent.setup();
  const Harness = () => {
    const [selected, setSelected] = useState<string | null>(null);
    return <Connections backend={backend} connectors={connectors} selected={selected} select={setSelected} changed={async () => {}} />;
  };
  render(<Harness />);
  await user.click(screen.getByRole("button", { name: "Connect Gmail" }));
  const file = new File([JSON.stringify({ installed: { client_id: "9-x.apps.googleusercontent.com", client_secret: "sec" } })], "client.json", { type: "application/json" });
  await user.upload(screen.getByLabelText("Import client file"), file);
  await screen.findByText("Client credentials saved for Gmail, Calendar and Drive");
  expect(save.mock.calls.map((c) => c[0]).sort()).toEqual(["gmail", "google-calendar", "google-drive"]);
  expect(save).toHaveBeenCalledWith("gmail", "9-x.apps.googleusercontent.com", "sec");
});

it("refuses a file that is not a Google OAuth client", async () => {
  const backend = new MockBackend();
  const connectors = await backend.listConnectors();
  const save = vi.spyOn(backend, "setClientCredentials");
  const user = userEvent.setup();
  const Harness = () => {
    const [selected, setSelected] = useState<string | null>(null);
    return <Connections backend={backend} connectors={connectors} selected={selected} select={setSelected} changed={async () => {}} />;
  };
  render(<Harness />);
  await user.click(screen.getByRole("button", { name: "Connect Gmail" }));
  await user.upload(screen.getByLabelText("Import client file"), new File(["{}"], "other.json", { type: "application/json" }));
  expect(await screen.findByRole("alert")).toHaveTextContent("isn't a Google OAuth client file");
  expect(save).not.toHaveBeenCalled();
});

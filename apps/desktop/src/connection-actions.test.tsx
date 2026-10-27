import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it, vi } from "vitest";
import { RunFlow } from "./RunFlow";
import { CreateFlow } from "./CreateFlow";
import { ReceiptCard } from "./ui";
import { MockBackend } from "./backend/mock";
import type { PlannedAction, Receipt, StagedRun } from "./backend/types";

const action: PlannedAction = {
  call: {connectorId:"gmail", toolName:"create_draft", args:{to:"test@example.invalid",body:'<script>alert("args")</script><img src=x onerror=alert(1)>'}},
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
  await user.click(screen.getByRole("button", {name:"Choose output folder…"}));
  await user.click(screen.getByRole("button", {name:"Stage and compare"}));
  expect(await screen.findByText(/Server’s claims: readOnlyHint = false; destructiveHint = false/)).toBeVisible();
  expect(screen.getByLabelText("Planned action 1").querySelector("pre")?.textContent).toBe(JSON.stringify(action.call.args, null, 2));
  const approve = screen.getByRole("button", {name:"Approve actions"});
  expect(approve).toBeDisabled();
  expect(commit).not.toHaveBeenCalled();
  await user.click(screen.getByRole("checkbox", {name:"I reviewed these actions"}));
  await user.click(approve);
  expect(commit).toHaveBeenCalledExactlyOnceWith("r", "hash", true);
  expect(await screen.findByText(receipt.actions![0].summary)).toBeVisible();
  expect(container.querySelectorAll("script,img,iframe,object")).toHaveLength(0);
  await user.click(screen.getByRole("button", {name:"Run again"}));
  await user.click(screen.getByRole("button", {name:"Stage and compare"}));
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
  await user.type(screen.getByLabelText("What do you want to do with this file?"), "draft a Gmail message");
  await user.click(screen.getByRole("button", {name:"Review task summary"}));
  const use = await screen.findByRole("checkbox", {name:`Let this tool use ${connector.name}`});
  expect(use).not.toBeChecked();
  await user.click(use);
  expect(await screen.findByText("create_draft")).toBeVisible();
  expect(select).toHaveBeenCalledWith(expect.any(String), [connector.id]);
  expect(screen.getByText(/<script>server instructions<\/script>/)).toBeVisible();
  expect(container.querySelectorAll("script,img,iframe")).toHaveLength(0);
});

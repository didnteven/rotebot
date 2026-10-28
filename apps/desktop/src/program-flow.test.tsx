import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it, vi } from "vitest";
import { CreateFlow } from "./CreateFlow";
import { RunFlow } from "./RunFlow";
import { MockBackend } from "./backend/mock";

it("creates a program, reviews real example text, compares inert output and saves", async () => {
  const backend = new MockBackend();
  await backend.saveApiKey("test-key");
  const input = backend.importText("application.log", "INFO start\nERROR <img src=x onerror=alert(1)>\nINFO done");
  const saved = vi.fn(async () => {});
  const user = userEvent.setup();
  const { container, unmount } = render(<CreateFlow backend={backend} initialInput={input} status={await backend.modelAccess()} connectors={[]} openSettings={vi.fn()} connect={vi.fn()} close={vi.fn()} saved={saved} />);
  const description = screen.getByLabelText("What should the tool do?");
  await user.clear(description);
  await user.type(description, "extract every line containing ERROR into errors.txt");
  await user.click(screen.getByRole("button", {name:"Check the plan"}));
  await user.click(screen.getByText("Details"));
  expect(await screen.findByLabelText("Task kind")).toHaveValue("program");
  expect(screen.queryByRole("region", {name:"Column mapping"})).toBeNull();
  await user.type(screen.getByLabelText("Example of the expected result (optional)"), "ERROR <img src=x onerror=alert(1)>");
  await waitFor(() => expect(screen.getByRole("button", {name:"Send and build"})).toBeEnabled());
  await user.click(screen.getByText(/What leaves this Mac ·/));
  expect(await screen.findByText("Your example result")).toBeVisible();
  expect(screen.getByText("ERROR <img src=x onerror=alert(1)>", {selector:"p"})).toBeVisible();
  expect(screen.getByLabelText("Exact data sent")).not.toBeVisible();
  await user.click(screen.getByText("Exact data sent"));
  expect(screen.getByLabelText("Exact data sent")).toBeVisible();
  const build = screen.getByRole("button", {name:"Send and build"});
  expect(build).toBeEnabled();
  await user.click(build);
  await waitFor(() => expect(screen.getByRole("button", {name:"Yes, save this tool"})).toBeVisible(), {timeout:5000});
  expect(screen.getByLabelText("Answer")).toHaveTextContent("ERROR <img src=x onerror=alert(1)>");
  expect(screen.getByRole("heading", {name:"Input · first lines", hidden:true})).not.toBeVisible();
  await user.click(screen.getByText("Details"));
  expect(container.querySelectorAll("pre").length).toBeGreaterThan(1);
  expect(container.querySelectorAll("img, script, iframe, object, svg")).toHaveLength(0);
  await user.click(screen.getByRole("button", {name:"Yes, save this tool"}));
  expect(saved).toHaveBeenCalledOnce();
  const tool = await backend.getTool((await backend.listTools())[0].id);
  expect(tool.engine).toBe("js-quickjs");
  // A malicious-looking source is displayed as text; the renderer never runs it.
  tool.program!.code = '<script>globalThis.programExecuted = true</script>';
  vi.spyOn(backend,"getTool").mockResolvedValue(tool);
  unmount();
  const view = render(<RunFlow backend={backend} tool={tool} close={vi.fn()} changed={async () => {}} />);
  expect(await screen.findByText("Program (sandboxed) · Version 1")).not.toBeVisible();
  await user.click(screen.getByText("Details"));
  expect(view.container.querySelector("pre")?.textContent).toContain("<script>");
  expect(view.container.querySelectorAll("script,img,iframe")).toHaveLength(0);
  expect((globalThis as Record<string,unknown>).programExecuted).toBeUndefined();
});

it("explains the missing model for programs on the offline route", async () => {
  const backend = new MockBackend();
  await backend.selectRoute("offline_demo");
  const input = backend.importText("synthetic.log", "ERROR invented");
  const user = userEvent.setup();
  render(<CreateFlow backend={backend} initialInput={input} status={await backend.modelAccess()} connectors={[]} openSettings={vi.fn()} connect={vi.fn()} close={vi.fn()} saved={async () => {}} />);
  await user.type(screen.getByLabelText("What should the tool do?"), "extract ERROR lines");
  await user.click(screen.getByRole("button",{name:"Check the plan"}));

  expect(await screen.findByText("Writing this tool needs a model: sign in to Muse or add a key in Settings.")).toBeVisible();
  expect(screen.getByRole("button",{name:"Send and build"})).toBeDisabled();
});

it("shows the sanitized runtime check detail when a program build fails", async () => {
  const backend = new MockBackend();
  await backend.saveApiKey("test-key");
  const input = backend.importText("synthetic.log", "ERROR invented");
  const detail = "JavaScript execution failed: ReferenceError at line 7, column 3: groupBy is not defined";
  vi.spyOn(backend, "startBuild").mockImplementation(async (draftId) => ({
    buildId: "runtime-error", draftId, route: "api_key", phase: "failed", events: [],
    candidate: null, comparison: null, spentMicros: null, routeLabel: "API key",
    checks: [{id: "sandbox", label: "Runs inside sandbox limits without error", passed: false, detail}],
    failure: {kind: "checks", message: `Checks failed: ${detail}. Edit the rules and build again.`},
  }));
  const user = userEvent.setup();
  render(<CreateFlow backend={backend} initialInput={input} status={await backend.modelAccess()} connectors={[]} openSettings={vi.fn()} connect={vi.fn()} close={vi.fn()} saved={async () => {}} />);
  await user.type(screen.getByLabelText("What should the tool do?"), "extract ERROR lines");
  await user.click(screen.getByRole("button", {name: "Check the plan"}));
  await waitFor(() => expect(screen.getByRole("button", {name:"Send and build"})).toBeEnabled());
  await user.click(screen.getByRole("button", {name: "Send and build"}));
  expect(await screen.findByRole("alert")).not.toHaveTextContent(detail);
  expect(screen.getByText(detail, {exact:false,selector:"li"})).not.toBeVisible();
  await user.click(screen.getByText("Details"));
  expect(screen.getByText(detail, {exact:false,selector:"li"})).toBeVisible();
  expect(screen.getByRole("button", {name: "Change the request"})).toBeVisible();
});

it("explains an example correction in plain words and keeps check shapes in one disclosure",async()=>{
  const backend=new MockBackend();await backend.saveApiKey("test-key");
  const input=backend.importText("synthetic.log","ERROR invented");
  const detail="Expected shape: ERROR #; received shape: error #";
  vi.spyOn(backend,"startBuild").mockImplementation(async(draftId)=>({
    buildId:"shape-error",draftId,route:"api_key",routeLabel:"Meta API key",phase:"failed",candidate:null,comparison:null,spentMicros:0,
    checks:[{id:"example",label:"Output shape matches",passed:false,detail}],
    events:[{buildId:"shape-error",seq:1,phase:"checking",atMs:1,message:"Checks failed; asking the model to correct the program (example format)"}],
    failure:{kind:"checks",message:detail},
  }));
  const user=userEvent.setup();
  const {container}=render(<CreateFlow backend={backend} initialInput={input} status={await backend.modelAccess()} connectors={[]} openSettings={vi.fn()} connect={vi.fn()} close={vi.fn()} saved={async()=>{}}/>);
  await user.type(screen.getByLabelText("What should the tool do?"),"extract errors");
  await user.click(screen.getByRole("button",{name:"Check the plan"}));

  await waitFor(() => expect(screen.getByRole("button", {name:"Send and build"})).toBeEnabled());
  await user.click(screen.getByRole("button",{name:"Send and build"}));
  expect(await screen.findByText("The output didn’t match your example’s format — Macroscope asked the model to fix it.")).not.toBeVisible();
  expect(container.querySelectorAll("details")).toHaveLength(1);
  expect(screen.getByText(detail,{selector:"details p"})).not.toBeVisible();
  await user.click(screen.getByText("Details"));
  expect(screen.getByText(detail,{selector:"details p"})).toBeVisible();
  expect(screen.getByText(/Output shape matches/)).toBeVisible();
});

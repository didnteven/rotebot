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
  const description = screen.getByLabelText("What do you want to do with this file?");
  await user.clear(description);
  await user.type(description, "extract every line containing ERROR into errors.txt");
  await user.click(screen.getByRole("button", {name:"Review task summary"}));
  expect(await screen.findByLabelText("Task kind")).toHaveValue("program");
  expect(screen.queryByRole("region", {name:"Column mapping"})).toBeNull();
  await user.type(screen.getByLabelText("Example of the expected result (optional)"), "ERROR <img src=x onerror=alert(1)>");
  await user.click(screen.getByRole("button", {name:"Review what leaves this Mac"}));
  expect(await screen.findByText(/Contains your real example text/)).toBeVisible();
  const build = screen.getByRole("button", {name:"Build tool"});
  expect(build).toBeDisabled();
  await user.click(screen.getByRole("checkbox"));
  await user.click(build);
  await waitFor(() => expect(screen.getByRole("button", {name:"Save tool"})).toBeVisible(), {timeout:5000});
  expect(screen.getByRole("heading", {name:"Input · first lines"})).toBeVisible();
  expect(screen.getByRole("heading", {name:"Output · first lines"})).toBeVisible();
  await user.click(screen.getByText("Advanced details"));
  expect(container.querySelectorAll("pre").length).toBeGreaterThan(2);
  expect(container.querySelectorAll("img, script, iframe, object, svg")).toHaveLength(0);
  await user.click(screen.getByRole("button", {name:"Save tool"}));
  expect(saved).toHaveBeenCalledOnce();
  const tool = await backend.getTool((await backend.listTools())[0].id);
  expect(tool.engine).toBe("js-quickjs");
  // A malicious-looking source is displayed as text; the renderer never runs it.
  tool.program!.code = '<script>globalThis.programExecuted = true</script>';
  vi.spyOn(backend,"getTool").mockResolvedValue(tool);
  unmount();
  const view = render(<RunFlow backend={backend} tool={tool} close={vi.fn()} changed={async () => {}} />);
  await screen.findByText("Program (sandboxed)");
  await user.click(screen.getByText("Advanced details"));
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
  await user.type(screen.getByLabelText("What do you want to do with this file?"), "extract ERROR lines");
  await user.click(screen.getByRole("button",{name:"Review task summary"}));
  await user.click(screen.getByRole("button",{name:"Review what leaves this Mac"}));
  expect(await screen.findByText("Writing a program needs a model: sign in to Muse or add a key")).toBeVisible();
  expect(screen.getByRole("button",{name:"Build tool"})).toBeDisabled();
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
  await user.type(screen.getByLabelText("What do you want to do with this file?"), "extract ERROR lines");
  await user.click(screen.getByRole("button", {name: "Review task summary"}));
  await user.click(screen.getByRole("button", {name: "Review what leaves this Mac"}));
  await user.click(await screen.findByRole("checkbox"));
  await user.click(screen.getByRole("button", {name: "Build tool"}));
  expect(await screen.findByRole("alert")).toHaveTextContent(detail);
  expect(screen.getByRole("button", {name: "Edit rules"})).toBeVisible();
});

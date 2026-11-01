import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it, vi } from "vitest";
import { App } from "./App";
import { CreateFlow } from "./CreateFlow";
import { MockBackend } from "./backend/mock";
import type { BuildState } from "./backend/types";

const button = (name: string) => screen.getByRole("button", { name });
async function create(result?: Partial<BuildState>) {
  const backend = new MockBackend();
  await backend.saveApiKey("test-key");
  const start = vi.spyOn(backend, "startBuild").mockImplementation(async draftId => ({
    buildId: "test-build", draftId, route: "api_key", routeLabel: "Meta API key",
    phase: "ready", candidate: null, checks: [], events: [], spentMicros: 0, failure: null,
    program: { summary: "Weather", output_name: "weather.txt", webSources: [], code: "function run() {}" },
    comparison: { inputText: "", outputText: "Auckland: fine, high 21°C.", inputColumns: [], inputRows: [], outputColumns: [], outputRows: [], rowsIn: 0, rowsOut: 1 },
    ...result,
  }));
  const save = vi.spyOn(backend, "saveTool").mockResolvedValue(await backend.installSampleTool());
  const saved = vi.fn(async () => {}), user = userEvent.setup();
  const props = { backend, initialInput: null, status: await backend.modelAccess(), connectors: [], openSettings: vi.fn(), connect: vi.fn(), close: vi.fn(), saved };
  const view = render(<CreateFlow {...props} />);
  await user.type(screen.getByLabelText("What should the tool do?"), "Show today's weather");
  await user.click(button("Check the plan"));
  // The data check and the suggested output layout run on their own; wait for both to settle.
  await screen.findByRole("button", { name: "Check again" });
  await waitFor(() => expect(button("Build and test")).toBeEnabled());
  return { backend, start, save, saved, user, props, ...view };
}

it("has three screens, focuses each heading, and records consent only on Send and build", async () => {
  const { start, user } = await create();
  expect(within(screen.getByRole("list", { name: "Create progress" })).getAllByRole("listitem")).toHaveLength(3);
  expect(screen.getByRole("heading", { name: "Check the plan" })).toHaveFocus();
  expect(screen.queryByRole("checkbox")).not.toBeInTheDocument();
  expect(screen.getByLabelText("Task kind")).not.toBeVisible();
  expect(screen.queryByRole("group", { name: "Connections this tool may use" })).not.toBeInTheDocument();
  expect(start).not.toHaveBeenCalled();
  await user.click(screen.getByText(/What gets sent ·/));
  await user.click(screen.getByText("Exact data sent", { selector: "summary" }));
  expect(screen.getByLabelText("Exact data sent")).toBeVisible();
  expect(start).not.toHaveBeenCalled();
  await user.click(button("Build and test"));
  expect(start).toHaveBeenCalledExactlyOnceWith(expect.any(String), { outboundReviewed: true });
  expect(screen.getByRole("heading", { name: "Result" })).toHaveFocus();
  expect(window.scrollTo).toHaveBeenLastCalledWith({ top: 0, behavior: "instant" });
  expect(screen.getByLabelText("Answer")).toHaveTextContent("Auckland: fine, high 21°C.");
  expect(screen.queryByText("0 rows → 1 row")).not.toBeInTheDocument();
  expect(screen.queryByText("Input · first lines")).not.toBeInTheDocument();
  await user.click(button("Back"));
  expect(screen.getByRole("heading", { name: "Check the plan" })).toHaveFocus();
  await user.click(button("Back"));
  expect(screen.getByRole("heading", { name: "What should it do?" })).toHaveFocus();
});

it("lists each site once in the Used line instead of asking for approval", async () => {
  const { user, save, saved } = await create({ program: {
    summary: "Stocks", code: "function run() {}", output_name: "stocks.txt", webSources: [
      { name: "stocks", url: "https://api.nasdaq.com/stocks" },
      { name: "prices", url: "https://api.nasdaq.com/prices" },
      { name: "other", url: "https://api.example.com/data" },
    ],
  } });
  await user.click(button("Build and test"));
  const used = screen.getByText(/^Used:/).closest("p")!;
  expect(used).toBeVisible();
  expect(used.textContent?.match(/api.nasdaq.com/g)).toHaveLength(1);
  expect(used).toHaveTextContent("Used: api.nasdaq.com · api.example.com");
  expect(screen.queryByText(/This tool will read data from/)).not.toBeInTheDocument();
  expect(button("Yes, save this tool")).toBeEnabled();
  expect(save).not.toHaveBeenCalled();
  await user.click(button("Yes, save this tool"));
  expect(save).toHaveBeenCalledOnce();
  expect(saved).toHaveBeenCalledOnce();
});

it("No, change it focuses the plan request and requires a refreshed plan before sending", async () => {
  const { user, start } = await create();
  await user.click(button("Build and test"));
  await user.click(button("No, change it"));
  const request = screen.getByLabelText("What should the tool do?");
  expect(request).toHaveFocus();
  fireEvent.change(request, { target: { value: "Show tomorrow's weather" } });
  expect(button("Build and test")).toBeDisabled();
  expect(screen.getByText("Update the plan to use your changed request.")).toBeVisible();
  await user.click(button("Update plan"));
  await waitFor(() => expect(button("Build and test")).toBeEnabled());
  expect(start).toHaveBeenCalledTimes(1);
  expect(screen.getByText(/The tool will show tomorrow's weather/)).toBeVisible();
});

it("friendly failure hides raw codes in Details and offers retry and a focused request change", async () => {
  const raw = "HTTP_403_FETCH: https://api.example.com refused the request..";
  const { user, start } = await create({ phase: "failed", comparison: null, failure: { kind: "internal", message: raw } });
  await user.click(button("Build and test"));
  expect(screen.getByRole("alert")).toHaveTextContent("Couldn't read data from api.example.com (the site refused the request). Try again, or change what the tool should do.");
  expect(screen.getByRole("alert")).not.toHaveTextContent("HTTP_403_FETCH");
  expect(screen.getByRole("alert")).not.toHaveTextContent("..");
  expect(screen.getByText(raw)).not.toBeVisible();
  await user.click(screen.getByText("Details"));
  expect(screen.getByText(raw)).toBeVisible();
  await user.click(button("Try again"));
  expect(start).toHaveBeenCalledTimes(2);
  await user.click(button("Change the request"));
  expect(screen.getByLabelText("What should the tool do?")).toHaveFocus();
});

it("tool list uses stored summary, friendly last run and Run now; duplicate names get UI suffixes", async () => {
  const backend = new MockBackend([{ id: "a", name: "Daily report", version: 1 }, { id: "b", name: "Daily report", version: 1 }]);
  const original = backend.getTool.bind(backend);
  vi.spyOn(backend, "getTool").mockImplementation(async id => ({ ...await original(id), summary: ["Summarizes the largest stock price change."] }));
  const tools = await backend.listTools();
  vi.spyOn(backend, "listTools").mockResolvedValue(tools.map((t, i) => ({ ...t, lastRunAtMs: i ? Date.now() : null })));
  render(<App backend={backend} />);
  expect(await screen.findByRole("heading", { name: "Daily report (1)" })).toBeVisible();
  expect(screen.getByRole("heading", { name: "Daily report (2)" })).toBeVisible();
  expect(await screen.findAllByText("Summarizes the largest stock price change.")).toHaveLength(2);
  expect(button("Run now: Daily report (1)")).toHaveTextContent("Run");
  expect(button("Run now: Daily report (2)")).toBeEnabled();
  expect(screen.getByText("Never run")).toBeVisible();
  expect(screen.getByText(/Ran today at/)).toBeVisible();
  expect((await original("a")).name).toBe("Daily report");
  expect(screen.getByRole("link", { name: "Skip to main content" })).toHaveAttribute("href", "#main-content");
});

it("a changed model setting sends retry back through the current plan and consent", async () => {
  const { user, backend, start, props, rerender } = await create({ phase: "failed", comparison: null, failure: { kind: "internal", message: "Error" } });
  await user.click(button("Build and test"));
  await backend.selectRoute("offline_demo");
  rerender(<CreateFlow {...props} status={await backend.modelAccess()} />);
  await user.click(button("Try again"));
  expect(start).toHaveBeenCalledTimes(1);
  expect(screen.getByRole("heading", { name: "Check the plan" })).toHaveFocus();
  await screen.findByText("Writing this tool needs a model: sign in to Muse or add a key in Settings.");
  expect(button("Build and test")).toBeDisabled();
});

it("a failed first request can be retried or changed without leaving an empty screen", async () => {
  const backend = new MockBackend();
  const draft = vi.spyOn(backend, "draftTaskNoInput").mockRejectedValueOnce(new Error("ERR_DRAFT"));
  const user = userEvent.setup();
  render(<CreateFlow backend={backend} initialInput={null} status={await backend.modelAccess()} connectors={[]} openSettings={vi.fn()} connect={vi.fn()} close={vi.fn()} saved={async () => {}} />);
  const request = screen.getByLabelText("What should the tool do?");
  await user.type(request, "Show the forecast");
  await user.click(button("Check the plan"));
  expect(await screen.findByRole("alert")).not.toHaveTextContent("ERR_DRAFT");
  await user.click(button("Change the request"));
  expect(request).toHaveFocus();
  expect(screen.getByRole("heading", { name: "What should it do?" })).toBeVisible();
  await user.click(button("Check the plan"));
  expect(draft).toHaveBeenCalledTimes(2);
  expect(screen.getByRole("heading", { name: "Check the plan" })).toBeVisible();
});

it("a failed data review can be retried without sending anything", async () => {
  const { backend, user, start } = await create();
  vi.spyOn(backend, "outboundPreview").mockRejectedValueOnce(new Error("ERR_PREVIEW"));
  await user.click(screen.getByText("Advanced"));
  fireEvent.change(screen.getByLabelText("Output file name"), { target: { value: "forecast.txt" } });
  expect(await screen.findByRole("alert")).not.toHaveTextContent("ERR_PREVIEW");
  expect(button("Build and test")).toBeDisabled();
  await user.click(button("Try again"));
  await waitFor(() => expect(button("Build and test")).toBeEnabled());
  expect(start).not.toHaveBeenCalled();
});

it("shows a loading state in Output while the AI writes the layout, then fills it in", async () => {
  const backend = new MockBackend();
  await backend.saveApiKey("test-key");
  const original = backend.suggestExampleOutput.bind(backend);
  let release!: () => void;
  vi.spyOn(backend, "suggestExampleOutput").mockImplementation(async id => { await new Promise<void>(r => { release = r; }); return original(id); });
  const user = userEvent.setup();
  render(<CreateFlow backend={backend} initialInput={null} status={await backend.modelAccess()} connectors={[]} openSettings={vi.fn()} connect={vi.fn()} close={vi.fn()} saved={async () => {}} />);
  await user.type(screen.getByLabelText("What should the tool do?"), "Get today's forecast for Auckland");
  await user.click(button("Check the plan"));
  expect(await screen.findByText("Writing the output layout…", { selector: "strong" })).toBeVisible();
  expect(screen.getByLabelText("What the result should look like")).toHaveAttribute("aria-busy", "true");
  expect(button("Build and test")).toBeDisabled();
  release();
  await waitFor(() => expect(screen.getByLabelText("What the result should look like")).toHaveValue("Auckland: fine, high 21C, low 14C."));
  expect(screen.queryByText("Writing the output layout…", { selector: "strong" })).not.toBeInTheDocument();
});

import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it } from "vitest";
import { App } from "./App";
import { MockBackend } from "./backend/mock";

const button = (name: string | RegExp) => screen.getByRole("button", { name });

it("creates a no-input weather tool with site approval, then stages and creates weather.txt", async () => {
  const backend = new MockBackend();
  await backend.saveApiKey("test-key");
  const user = userEvent.setup();
  render(<App backend={backend} />);
  await waitFor(() => expect(button("Describe a task")).toBeEnabled());

  await user.click(button("Describe a task"));
  await user.click(button("No file"));
  expect(
    screen.getByText(/No file selected/),
  ).toBeVisible();
  const description = screen.getByLabelText(
    "What should the tool do?",
  );
  await user.clear(description);
  await user.type(
    description,
    "Get today's forecast for Auckland from https://api.open-meteo.com and write a short summary to weather.txt",
  );
  await user.click(button("Check the plan"));

  await waitFor(() => expect(button("Send and build")).toBeEnabled());
  await user.click(screen.getByText(/What leaves this Mac ·/));

  // Before generation, requested hosts are named; exact sources need approval.
  expect(
    await screen.findByText(/Requested site: api.open-meteo.com on each run/),
  ).toBeVisible();
  expect(button("Send and build")).toBeEnabled();
  await user.click(button("Send and build"));

  // The build-and-compare review lists the declared hosts; saving requires approval.
  await waitFor(() => expect(button("Yes, save this tool")).toBeVisible(), {
    timeout: 5000,
  });
  expect(
    screen.getByText(/This tool will read data from api.open-meteo.com/),
  ).toBeVisible();
  expect(button("Yes, save this tool")).toBeEnabled();
  const toolName = screen.getByLabelText("Tool name");
  await user.clear(toolName);
  await user.type(toolName, "Weather");
  await user.click(button("Yes, save this tool"));

  await screen.findByRole("heading", { name: "Weather" });
  const tool = (await backend.listTools()).find((t) => t.name === "Weather")!;
  expect(tool.engine).toBe("js-quickjs");
  const detail = await backend.getTool(tool.id);
  expect(detail.noInput).toBe(true);
  expect(detail.program?.webSources).toHaveLength(1);

  // The run screen says which sites will be fetched; no input file is asked for.
  await user.click(button("Run now: Weather"));
  expect(
    await screen.findByText(/This run reads data from api.open-meteo.com/),
  ).toBeVisible();
  expect(
    screen.queryByRole("button", { name: "Choose input file" }),
  ).not.toBeInTheDocument();
  await user.click(button("Choose output folder"));
  await user.click(button("Preview result"));
  await screen.findByRole("heading", { name: "Result preview" });
  expect(
    screen.getByLabelText("Answer"),
  ).toHaveTextContent("Auckland: fine, high 21C, low 14C.");
  await user.click(button("Create previewed output"));
  expect(
    await screen.findByRole("heading", { name: "Run receipt" }),
  ).toBeVisible();
  expect(screen.getByText("/Demo/Reports/weather.txt")).toBeVisible();

  // Every fetch is recorded for Privacy.
  const outbound = (await backend.privacyOverview()).outbound;
  expect(
    outbound.some(
      (o) => o.purpose === "Web source" && o.destination === "api.open-meteo.com",
    ),
  ).toBe(true);
});

it("blocks web fetches under Local only in the mock backend", async () => {
  const backend = new MockBackend();
  await backend.saveApiKey("test-key");
  const draft = await backend.draftTaskNoInput("weather");
  await backend.updateTask(draft.draftId, "program", "weather.txt", null);
  const build = await backend.startBuild(draft.draftId, {
    outboundReviewed: true,
  });
  for (let i = 0; i < 40 && (await backend.buildState(build.buildId)).phase !== "ready"; i++)
    await new Promise((resolve) => setTimeout(resolve, 100));
  const ready = await backend.buildState(build.buildId);
  expect(ready.phase).toBe("ready");
  const tool = await backend.saveTool(build.buildId, "Weather");
  await backend.setLocalOnly(true);
  const result = await backend.stageRunNoInput(
    tool.id,
    await backend.pickOutputDir().then((d) => d!),
    "weather.txt",
  );
  expect("failure" in result && result.failure).toMatchObject({
    kind: "internal",
    message: expect.stringContaining("Local only"),
  });
});

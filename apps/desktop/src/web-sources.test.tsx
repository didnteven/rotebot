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
  await user.click(button("No input file"));
  expect(
    screen.getByText(/No input file — the program will read approved web sources/),
  ).toBeVisible();
  const description = screen.getByLabelText(
    "What do you want to do with this file?",
  );
  await user.clear(description);
  await user.type(
    description,
    "Get today's forecast for Auckland from https://api.open-meteo.com and write a short summary to weather.txt",
  );
  await user.click(button("Review task summary"));

  expect(
    await screen.findByText(/No input file: only a sandboxed program/),
  ).toBeVisible();
  await user.click(button("Review what leaves this Mac"));

  // Before generation, requested hosts are named; exact sources need approval.
  expect(
    await screen.findByText(/Requested site: api.open-meteo.com on each run/),
  ).toBeVisible();
  expect(button("Build tool")).toBeDisabled();
  await user.click(
    screen.getByRole("checkbox", { name: "I've reviewed what will be sent" }),
  );
  await user.click(button("Build tool"));

  // The build-and-compare review lists the declared hosts; saving requires approval.
  await waitFor(() => expect(button("Save tool")).toBeVisible(), {
    timeout: 5000,
  });
  expect(
    screen.getByText(/This tool fetches from api.open-meteo.com/),
  ).toBeVisible();
  expect(button("Save tool")).toBeDisabled();
  await user.click(
    screen.getByRole("checkbox", {
      name: "I approve fetching from these sites",
    }),
  );
  expect(button("Save tool")).toBeEnabled();
  const toolName = screen.getByLabelText("Tool name");
  await user.clear(toolName);
  await user.type(toolName, "Weather");
  await user.click(button("Save tool"));

  await screen.findByRole("heading", { name: "Weather" });
  const tool = (await backend.listTools()).find((t) => t.name === "Weather")!;
  expect(tool.engine).toBe("js-quickjs");
  const detail = await backend.getTool(tool.id);
  expect(detail.noInput).toBe(true);
  expect(detail.program?.webSources).toHaveLength(1);

  // The run screen says which sites will be fetched; no input file is asked for.
  await user.click(button("Run Weather…"));
  expect(
    await screen.findByText(/This run fetches from api.open-meteo.com/),
  ).toBeVisible();
  expect(
    screen.queryByRole("button", { name: "Choose input file…" }),
  ).not.toBeInTheDocument();
  await user.click(button("Choose output folder…"));
  await user.click(button("Stage and compare"));
  await screen.findByRole("heading", { name: "Staged preview" });
  expect(
    screen.getByLabelText("Staged output first lines"),
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

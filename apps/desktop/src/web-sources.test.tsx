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
  const description = screen.getByLabelText(
    "What should the tool do?",
  );
  await user.clear(description);
  await user.type(
    description,
    "Get today's forecast for Auckland from https://api.open-meteo.com and write a short summary to weather.txt",
  );
  await user.click(button("Check the plan"));

  await waitFor(() => expect(button("Build and test")).toBeEnabled());
  await user.click(screen.getByText(/What gets sent ·/));

  // Before generation, requested hosts are named; exact sources need approval.
  expect(
    await screen.findByText(/Requested site: api.open-meteo.com on each run/),
  ).toBeVisible();
  expect(button("Build and test")).toBeEnabled();
  await user.click(button("Build and test"));

  // The result lists the sites the tool used; saving needs no separate approval.
  await waitFor(() => expect(button("Yes, save this tool")).toBeVisible(), {
    timeout: 5000,
  });
  expect(
    screen.getByText(/api.open-meteo.com/, { selector: "p.used-line" }),
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

  // No input file is asked for; the result says which site was used.
  await user.click(button("Run now: Weather"));
  await screen.findByRole("button", { name: "Preview result" });
  expect(
    screen.queryByRole("button", { name: "Choose input file" }),
  ).not.toBeInTheDocument();
  // The answer can be read before any folder or file name is chosen.
  expect(
    screen.queryByRole("button", { name: "Choose output folder" }),
  ).not.toBeInTheDocument();
  await user.click(button("Preview result"));
  expect(
    await screen.findByLabelText("Answer"),
  ).toHaveTextContent("Auckland: fine, high 21C, low 14C.");
  expect(screen.getByText(/api.open-meteo.com/, { selector: "p.used-line" })).toBeVisible();
  await user.click(button("Choose output folder"));
  await user.click(button("Create file"));
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

import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { App } from "./App";
import { MockBackend } from "./backend/mock";

const button = (name: string | RegExp) => screen.getByRole("button", { name });
async function open(backend = new MockBackend()) {
  const user = userEvent.setup();
  render(<App backend={backend} />);
  await waitFor(() => expect(button("Describe a task")).toBeEnabled());
  return { user, backend };
}
async function summary(user: ReturnType<typeof userEvent.setup>) {
  await user.click(button("Try a sample"));
  await user.click(button("Check the plan"));
  await user.click(screen.getByRole("radio", { name: "March 4 (MM/DD/YY)" }));
  await waitFor(() => expect(screen.queryByText("Preparing what will be sent…")).not.toBeInTheDocument());
  await user.click(screen.getByText("Output"));
  await user.click(screen.getByText("Advanced"));
}
async function review(user: ReturnType<typeof userEvent.setup>) {
  await summary(user);
  await user.click(screen.getByText(/What gets sent ·/));
}
async function savedTool() {
  const backend = new MockBackend();
  await backend.selectRoute("offline_demo");
  const { user } = await open(backend);
  await review(user);
  expect(button("Build and test")).toBeEnabled();
  expect(screen.queryByRole("checkbox")).not.toBeInTheDocument();
  await user.click(button("Build and test"));
  await waitFor(() => expect(button("Yes, save this tool")).toBeVisible(), {
    timeout: 4500,
  });
  await user.click(screen.getByText("Details"));
  expect(
    screen.getByText(/✓ Pass · Row count preserved · 18 → 18/),
  ).toBeVisible();
  await user.click(button("Yes, save this tool"));
  await screen.findByRole("heading", { name: "Clean import CSV Sample" });
  return { user, backend };
}
async function stage(user: ReturnType<typeof userEvent.setup>) {
  await user.click(button("Use sample input"));
  await user.click(button("Choose output folder"));
  await user.click(button("Preview result"));
  await screen.findByRole("heading", { name: "Result preview" });
}
afterEach(() => vi.useRealTimers());

describe("Tell and deterministic runs", () => {
  it("answers, reviews, builds, saves, stages and creates the previewed CSV", async () => {
    const { user, backend } = await savedTool();
    await user.click(button("Run now: Clean import CSV"));
    await stage(user);
    await user.click(button("Create previewed CSV"));
    expect(
      await screen.findByRole("heading", { name: "Run receipt" }),
    ).toBeVisible();
    expect(screen.getByText("/Demo/Reports/orders_ready.csv")).toBeVisible();
    expect((await backend.listReceipts())[0].status).toBe("committed");
    await user.click(button("Activity"));
    expect(
      await screen.findByRole("heading", { name: "Run receipt" }),
    ).toBeVisible();
  });
  it("refuses an existing destination and lets the user change the name", async () => {
    const backend = new MockBackend();
    await backend.installSampleTool();
    const { user } = await open(backend);
    await user.click(button("Run now: Clean import CSV"));
    await stage(user);
    await user.click(button("Create previewed CSV"));
    await screen.findByRole("heading", { name: "Run receipt" });
    await user.click(button("Run again"));
    await user.click(button("Preview result"));
    await user.click(button("Create previewed CSV"));
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "already exists",
    );
    await user.click(button("Change the request"));
    const name = screen.getByLabelText("Output name");
    await user.clear(name);
    await user.type(name, "orders_second.csv");
    await user.click(button("Preview result"));
    await user.click(button("Create previewed CSV"));
    expect(
      await screen.findByText("/Demo/Reports/orders_second.csv"),
    ).toBeVisible();
    expect(await backend.listReceipts()).toHaveLength(2);
  });
  it("stops an active build and never presents it as ready", async () => {
    const backend = new MockBackend();
    await backend.selectRoute("offline_demo");
    const { user } = await open(backend);
    await review(user);
    await user.click(button("Build and test"));
    await user.click(await screen.findByRole("button", { name: "Stop build" }));
    expect(
      await screen.findByText(/Build stopped\. Your plan is saved\. Change/),
    ).toBeVisible();
    expect(
      screen.queryByRole("button", { name: "Yes, save this tool" }),
    ).not.toBeInTheDocument();
  });
  it("shows no empty program or mapping in Details while the first candidate is still being written", async () => {
    const backend = new MockBackend();
    await backend.selectRoute("offline_demo");
    const { user } = await open(backend);
    await review(user);
    await user.click(button("Build and test"));
    await screen.findByRole("button", { name: "Stop build" });
    const details = screen.getByText("Details").closest("details")!;
    expect(details).not.toHaveTextContent("CSV mapping");
    expect(details).not.toHaveTextContent("Program (sandboxed)");
    expect(details.querySelector("pre")).toBeNull();
    expect(details).not.toHaveTextContent(/\bnull\b/);
  });
  it("explains the automatic offline route and preserves the draft across settings navigation", async () => {
    const { user } = await open();
    await review(user);
    expect(
      screen.getByText(
        "Nothing leaves this Mac. The offline demo maps columns without a model; results are labelled as a demo.",
      ),
    ).toHaveClass("notice-info");
    expect(button("Build and test")).toBeEnabled();
    await user.click(button("Settings"));
    const route = screen.getByRole("radio", {
      name: "Offline demo (samples only)",
    });
    expect(route).toBeChecked();
    await user.click(route);
    await user.click(button("My tools"));
    expect(
      screen.getByRole("heading", { name: "Check the plan" }),
    ).toBeVisible();
    expect(button("Build and test")).toBeEnabled();
  });
  it("builds a fresh mock with no key, no Muse sign-in and no selected route to ready", async () => {
    const { user, backend } = await open();
    expect(await backend.modelAccess()).toMatchObject({
      apiKey: { present: false },
      muse: { kind: "not_signed_in" },
      selectedRoute: null,
      effectiveRoute: "offline_demo",
      blockedReason: expect.any(String),
    });
    await review(user);
    expect(screen.getByText(/Nothing leaves this Mac\. The offline demo/, {selector:"p"})).toBeVisible();
    expect(screen.getByLabelText("Exact data sent")).not.toBeVisible();
    expect(
      screen.queryByText("Contains only synthetic rows"),
    ).not.toBeInTheDocument();
    expect(screen.queryByText(/Budget:/)).not.toBeInTheDocument();
    expect(screen.queryByRole("checkbox")).not.toBeInTheDocument();
    expect(button("Build and test")).toBeEnabled();
    await user.click(button("Build and test"));
    await waitFor(() => expect(button("Yes, save this tool")).toBeVisible(), {
      timeout: 4500,
    });
    expect(screen.queryByText(/^Used:/)).not.toBeInTheDocument();
    expect((await backend.privacyOverview()).outbound).toEqual([]);
  });
  it("requires outbound consent for cloud builds and preserves the full review", async () => {
    const backend = new MockBackend();
    await backend.saveApiKey("test-key");
    const { user } = await open(backend);
    await review(user);
    expect(screen.getByLabelText("Exact data sent")).not.toBeVisible();
    await user.click(screen.getByText("Exact data sent"));
    expect(screen.getByLabelText("Exact data sent")).toBeVisible();
    expect(screen.getByText("A made-up sample of your data's shape (no real rows)")).toBeVisible();
    expect(screen.getByText(/Estimated cost:.*Budget:/)).toBeVisible();
    expect(screen.queryByRole("checkbox", { name: "I've reviewed what will be sent" })).not.toBeInTheDocument();
    expect(button("Build and test")).toBeEnabled();
    await user.click(button("Build and test"));
    await waitFor(() => expect(button("Yes, save this tool")).toBeVisible(), {
      timeout: 4500,
    });
  });
  it("shows invalid input row, column and fix action", async () => {
    const backend = new MockBackend();
    await backend.installSampleTool();
    vi.spyOn(backend, "pickCsv").mockResolvedValue(
      backend.importCsv(
        "bad.csv",
        "order_id,created,amount\nORD,99/99/26,$1.00",
      ),
    );
    const { user } = await open(backend);
    await user.click(button("Run now: Clean import CSV"));
    await user.click(button("Choose input file"));
    await user.click(button("Choose output folder"));
    await user.click(button("Preview result"));
    expect(await screen.findByRole("alert")).toHaveTextContent("Couldn't read this file");
    await user.click(screen.getByText("File details"));
    expect(screen.getByText("Row: 1 · Column: created")).toBeVisible();
    expect(button("Try again")).toBeVisible();
    expect(button("Change the request")).toBeVisible();
  });
  it("renders malicious cells as literal text, with no image or script", async () => {
    const backend = new MockBackend();
    vi.spyOn(backend, "pickCsv").mockResolvedValue(
      backend.importCsv(
        "unsafe.csv",
        "order_id,created,amount,customer\nORD,03/04/26,$1.00,<img src=x onerror=alert(1)>",
      ),
    );
    const { user } = await open(backend);
    await user.click(button("Describe a task"));
    await user.click(button("+ Add a file"));
    expect(
      await screen.findByText("<img src=x onerror=alert(1)>"),
    ).toBeVisible();
    expect(document.querySelector("main img")).toBeNull();
    expect(document.querySelector("main script")).toBeNull();
  });
  it("detects mentioned services and keeps mapping edits across navigation", async () => {
    const { user } = await open();
    await user.click(button("Try a sample"));
    const text = screen.getByLabelText("What should the tool do?");
    await user.clear(text);
    await user.type(text, "Clean Gmail orders, then Slack and notion.");
    await user.click(button("Check the plan"));
    expect(screen.getByText(/You mentioned Gmail/)).toBeVisible();
    await user.click(screen.getByText("Output"));
    const column = screen.getByLabelText("Output column 1");
    await user.clear(column);
    await user.type(column, "Order reference");
    await user.click(button("Connect Gmail"));
    await user.click(button("My tools"));
    expect(screen.getByLabelText("Output column 1")).toHaveValue(
      "Order reference",
    );
  });
});

it("opens Record a task from My tools and returns to the list", async () => {
  const { user } = await open();
  await user.click(button("Record a task"));
  expect(screen.getByRole("heading", { name: "Show Rotebot what to do" })).toBeVisible();
  await user.click(button("Back"));
  expect(button("Describe a task")).toBeVisible();
});

describe("Settings, connections, Show and privacy", () => {
  it("saves and clears the key, selects a route, and refreshes simulated Muse sign-in", async () => {
    const { user, backend } = await open();
    await user.click(button("Settings"));
    await user.click(button("Add key…"));
    const key = screen.getByLabelText("Paste your Meta API key");
    await user.type(key, "demo-secret-abcd");
    await user.click(button("Save key"));
    expect(key).toHaveValue("");
    expect(await screen.findByText("Key ending …abcd")).toBeVisible();
    await user.click(
      screen.getByRole("radio", {
        name: "Meta API key",
      }),
    );
    expect((await backend.modelAccess()).selectedRoute).toBe("api_key");
    await user.click(button("Sign in…"));
    await user.click(button("Sign in with Muse…"));
    expect(
      await screen.findByText(
        "Finish signing in in Terminal, then check again",
      ),
    ).toBeVisible();
    await new Promise((resolve) => setTimeout(resolve, 750));
    await user.click(button("Check again"));
    expect(await screen.findByText("Signed in")).toBeVisible();
    await user.click(button("Remove key"));
    await user.click(
      screen.getByRole("radio", {
        name: "Muse plan",
      }),
    );
    expect((await backend.modelAccess()).effectiveRoute).toBe("muse_plan");
  });
  it("uses native key entry only for the native macOS backend", async () => {
    const backend = new MockBackend();
    vi.spyOn(backend, "appInfo").mockResolvedValue({
      platform: "macos",
      version: "0.1.0",
      mock: false,
      secretStore: "keychain",
      debugBuild: false,
    });
    const native = vi
      .spyOn(backend, "enterApiKeyNative")
      .mockImplementation(() => backend.saveApiKey("native-abcd"));
    const { user } = await open(backend);
    await user.click(button("Settings"));
    await user.click(button("Add key…"));
    expect(native).toHaveBeenCalledOnce();
    expect(screen.queryByLabelText("Paste your Meta API key")).not.toBeInTheDocument();
    expect(await screen.findByText("Key ending …abcd")).toBeVisible();
  });
  it("requires Gmail credentials, clears them, then connects, tests and disconnects", async () => {
    const { user, backend } = await open();
    await user.click(button("Connections"));
    await user.click(button("Connect Gmail"));
    expect(button("Connect")).toBeDisabled();
    await expect(backend.beginConnect("gmail")).rejects.toThrow(
      "requires a client ID",
    );
    const id = screen.getByLabelText("Client ID"),
      secret = screen.getByLabelText("Client secret");
    expect(id).toHaveAttribute("type", "password");
    expect(secret).toHaveAttribute("type", "password");
    await user.type(id, "client-id");
    await user.type(secret, "client-secret");
    await user.click(button("Save client credentials"));
    expect(id).toHaveValue("");
    expect(secret).toHaveValue("");
    await user.click(button("Connect"));
    expect(
      await screen.findByText("Finish signing in in your browser…"),
    ).toBeVisible();
    expect(
      (screen.getByLabelText("Authorization URL") as HTMLInputElement).value,
    ).toMatch(/^https:\/\/example\.invalid\/authorize\?/);
    expect(
      await screen.findByText(
        "Connected · 12 tools available",
        {},
        { timeout: 2500 },
      ),
    ).toBeVisible();
    await user.click(button("Test"));
    expect(await screen.findByText("Connection test passed")).toBeVisible();
    await user.click(button("Disconnect"));
    await waitFor(() => expect(button("Connect")).toBeVisible());
  });
  it("cancels an in-flight connection without reconnecting", async () => {
    const { user, backend } = await open();
    await user.click(button("Connections"));
    // Choosing a service that can sign in starts its browser sign-in at once.
    await user.click(button("Connect Notion"));
    await user.click(await screen.findByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(button("Connect")).toBeEnabled(), {
      timeout: 2500,
    });
    expect(
      (await backend.listConnectors()).find((c) => c.id === "notion")?.state
        .kind,
    ).toBe("not_connected");
  });
  it("records, pauses, stops, summarizes and dismisses a connection suggestion", async () => {
    const { user } = await open();
    await user.click(button("Record a task"));
    await user.click(screen.getByText("Advanced: app activity log"));
    await user.click(button("Start recording"));
    expect(await screen.findByRole("timer")).toHaveTextContent("0:00 / 5:00");
    await user.click(button("Pause"));
    expect(button("Resume")).toBeVisible();
    await user.click(button("Resume"));
    await user.click(button("Stop"));
    expect(
      await screen.findByRole("heading", { name: "Recording summary" }),
    ).toBeVisible();
    expect(screen.getByText("Apps seen: Google Chrome, Numbers")).toBeVisible();
    expect(
      await screen.findByText(
        "You used Gmail while recording. Connect Gmail so tools can work with it.",
      ),
    ).toBeVisible();
    await user.click(button("Not now"));
    expect(
      screen.queryByText(
        "You used Gmail while recording. Connect Gmail so tools can work with it.",
      ),
    ).not.toBeInTheDocument();
    await user.click(button("Connections"));
    expect(
      screen.queryByRole("button", { name: "Not now" }),
    ).not.toBeInTheDocument();
  });
  it("shows and connects a suggestion in Connections after recording", async () => {
    const { user } = await open();
    await user.click(button("Record a task"));
    await user.click(screen.getByText("Advanced: app activity log"));
    await user.click(button("Start recording"));
    await user.click(button("Stop"));
    await user.click(button("Connections"));
    const banner = screen
      .getByText(
        "You used Gmail while recording. Connect Gmail so tools can work with it.",
      )
      .closest("section")!;
    await user.click(
      within(banner).getByRole("button", { name: "Connect Gmail" }),
    );
    expect(
      screen.getByRole("region", { name: "Gmail connection details" }),
    ).toBeVisible();
  });
  it("changes egress, stops Show, and deletes outbound history", async () => {
    const backend = new MockBackend();
    await backend.saveApiKey("test-key");
    const csv = await backend.loadSampleCsv();
    const d = await backend.draftTask(csv.handle, "Clean orders");
    await backend.answerQuestion(d.draftId, "date-format", "mm_dd_yy");
    await backend.startBuild(d.draftId, { outboundReviewed: true });
    const { user } = await open(backend);
    await user.click(button("Settings"));
  await user.click(screen.getByRole("tab", { name: "Privacy & data" }));
    await user.click(await screen.findByText("Stored data and sent data history"));
    expect(await screen.findByText("Build CSV tool")).toBeVisible();
    await user.click(screen.getByRole("radio", { name: "Local only" }));
    expect(await screen.findByRole("status")).toHaveTextContent(
      "Stopping cloud work…",
    );
    await waitFor(() =>
      expect(screen.getByRole("radio", { name: "Local only" })).toBeChecked(),
    );
    await user.click(button("Delete sent data history"));
    expect(await screen.findByText("No sent data history.")).toBeVisible();
    await backend.startShow();
    await user.click(button("Emergency stop"));
    await waitFor(async () =>
      expect((await backend.showState()).kind).toBe("finished"),
    );
    expect((await backend.modelAccess()).egress).toBe("local_only");
  });
  it("updates local service matches after three seconds", async () => {
    vi.useFakeTimers();
    const backend = new MockBackend();
    const events: unknown[] = [];
    const off = await backend.onShowEvent((s) => events.push(s));
    await backend.startShow();
    await act(async () => vi.advanceTimersByTimeAsync(3000));
    expect(await backend.showState()).toMatchObject({
      kind: "recording",
      matches: 1,
      appsSeen: 1,
    });
    await act(async () => vi.advanceTimersByTimeAsync(297000));
    expect((await backend.showState()).kind).toBe("finished");
    expect(events).toHaveLength(3);
    off();
  });
});

it("shows mapping labels while sending enum values to the backend", async () => {
  const { user, backend } = await open();
  const update = vi.spyOn(backend, "updateMapping");
  await summary(user);
  expect(
    within(screen.getByLabelText("Transform 1")).getByRole("option", {
      name: "Copy as is",
    }),
  ).toHaveValue("copy");
  expect(
    within(screen.getByLabelText("Transform 1")).getByRole("option", {
      name: "Trim spaces",
    }),
  ).toHaveValue("trim");
  for (const label of [
    "From format",
    "To format",
    "Currency",
    "Decimal places",
  ])
    expect(screen.getByText(label)).toBeVisible();
  await user.selectOptions(
    screen.getByLabelText("Date output format 2"),
    "yyyy_mm_dd",
  );
  await user.selectOptions(screen.getByLabelText("Transform 1"), "constant");
  expect(screen.getByText("Value")).toBeVisible();
  await user.type(screen.getByLabelText("Constant value 1"), "Fixed");
  await waitFor(() => expect(button("Build and test")).toBeEnabled());
  expect(update).toHaveBeenLastCalledWith(
    expect.any(String),
    expect.objectContaining({
      output_columns: expect.arrayContaining([
        expect.objectContaining({
          transform: { type: "constant", value: "Fixed" },
        }),
        expect.objectContaining({
          transform: { type: "date", from: "mm_dd_yy", to: "yyyy_mm_dd" },
        }),
      ]),
    }),
    "orders_ready.csv",
  );
});

it("loads connector tools only on disclosure and renders server text safely", async () => {
  const { user, backend } = await open();
  const preview = vi.spyOn(backend, "previewConnectorTools");
  await user.click(button("Connections"));
  expect(preview).not.toHaveBeenCalled();
  const gmail = button("Connect Gmail").closest("article")!;
  await user.click(button("Connect Gmail"));
  await user.click(within(gmail).getByText("What it can do"));
  expect(await within(gmail).findByText("Create draft email")).toBeVisible();
  expect(preview).toHaveBeenCalledExactlyOnceWith("gmail");
  await user.click(within(gmail).getByText("What it can do"));
  await user.click(within(gmail).getByText("What it can do"));
  expect(preview).toHaveBeenCalledTimes(1);
  const notion = button("Connect Notion").closest("article")!;
  await user.click(button("Connect Notion"));
  await user.click(within(notion).getByText("What it can do"));
  expect(
    await within(notion).findByText("Sign in to see its tools"),
  ).toBeVisible();
  preview.mockResolvedValueOnce([
    {
      name: "<img src=x onerror=alert(1)>",
      description: "<script>untrusted</script>",
      readOnly: null,
      destructive: null,
    },
  ]);
  await user.click(screen.getByText("Advanced"));
  const custom = button("Connect Custom MCP server").closest("article")!;
  await user.click(button("Connect Custom MCP server"));
  await user.click(within(custom).getByText("What it can do"));
  expect(
    await within(custom).findByText("<img src=x onerror=alert(1)>"),
  ).toBeVisible();
  expect(custom.querySelector("img, script")).toBeNull();
});

it("opens connector docs through the backend and copies the registered redirect URI", async () => {
  const { user, backend } = await open();
  const external = vi.spyOn(backend, "openExternal").mockResolvedValue();
  await user.click(button("Connections"));
  await user.click(button("Connect Gmail"));
  await user.click(button("Documentation"));
  expect(external).not.toHaveBeenCalled();
  expect(screen.getByRole("region", { name: "Open documentation" })).toBeVisible();
  await user.click(button("Open documentation in browser"));
  expect(external).toHaveBeenCalledExactlyOnceWith(
    "https://developers.google.com/workspace/gmail/api/guides",
  );
  await user.click(button("Connect Slack"));
  const slack = (await backend.listConnectors()).find((c) => c.id === "slack")!;
  expect(screen.getByText(slack.clientSetup!)).toBeVisible();
  expect(screen.getByLabelText("Redirect URI")).toHaveValue(
    "http://127.0.0.1:53682/callback",
  );
  await user.click(button("Copy"));
  expect(await navigator.clipboard.readText()).toBe(
    "http://127.0.0.1:53682/callback",
  );
});

it("keeps the cloud route blocked when effectiveRoute is null", async () => {
  const backend = new MockBackend();
  await backend.saveApiKey("test-key");
  const status = await backend.modelAccess();
  vi.spyOn(backend, "modelAccess").mockResolvedValue({
    ...status,
    effectiveRoute: null,
    blockedReason: "Key unavailable",
  });
  const { user } = await open(backend);
  await review(user);
  expect(button("Build and test")).toBeDisabled();
  expect(screen.getByText("Choose a model in Settings to build this tool.")).toBeVisible();
  expect(button("Open model settings").closest("div")).toHaveClass(
    "notice-warning",
  );
});

it("keeps privacy to two controls and shows no size for empty data", async () => {
  const { user } = await open();
  await user.click(button("Settings"));
  await user.click(screen.getByRole("tab", { name: "Privacy & data" }));
  const region = await screen.findByRole("region", {
    name: "Data stored by Rotebot",
  });
  expect(screen.getByRole("group", { name: "Cloud models" })).toBeVisible();
  expect(document.querySelector("dl.explanations")).toBeNull();
  for (const row of within(region).getAllByRole("row").slice(1)) {
    const cells = within(row).getAllByRole("cell");
    if (cells[1].textContent === "0") expect(cells[2]).toHaveTextContent("—");
  }
});

it("URL edits ask for an OAuth client ID again", async () => {
  const { user } = await open();
  await user.click(button("Connections"));
  await user.click(button("Connect Gmail"));
  await user.type(screen.getByLabelText("Client ID"), "old-client-id");
  await user.type(screen.getByLabelText("Client secret"), "old-client-secret");
  await user.click(button("Save client credentials"));
  expect(await screen.findByText("✓ Client credentials saved")).toBeVisible();
  expect(button("Connect")).toBeEnabled();
  await user.clear(screen.getByLabelText("MCP URL"));
  await user.type(screen.getByLabelText("MCP URL"), "https://new.example/mcp");
  await user.click(button("Save MCP URL"));
  expect(await screen.findByText("Add your OAuth client ID")).toBeVisible();
  expect(button("Connect")).toBeDisabled();
  expect(screen.getByLabelText("Client ID")).toHaveValue("");
  expect(screen.getByLabelText("Client secret")).toHaveValue("");
});

import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, expect, it, vi } from "vitest";
import { App } from "./App";
import { MockBackend } from "./backend/mock";
import { sampleCsv } from "./backend/sample";

const button = (name: string | RegExp) => screen.getByRole("button", { name });
afterEach(() => vi.useRealTimers());

it("previews history, defaults to keeping tools and credentials, and lets Keep cancel", async () => {
  const backend = new MockBackend();
  await backend.installSampleTool();
  await backend.saveApiKey("keep-this-key");
  const remove = vi.spyOn(backend, "deleteData");
  const user = userEvent.setup();
  render(<App backend={backend} />);
  await screen.findByRole("button", { name: "Export…" });
  await user.click(button("Privacy"));
  await screen.findByRole("table", { name: "Local history to delete" });
  expect(
    screen.getByText(/Your API key and connection tokens stay/),
  ).toBeVisible();
  await user.click(button("Delete local history…"));
  expect(
    screen.getByRole("checkbox", { name: "Also delete my tools" }),
  ).not.toBeChecked();
  await user.click(button("Keep"));
  expect(remove).not.toHaveBeenCalled();
  await user.click(button("Delete local history…"));
  await user.click(button("Delete history"));
  await screen.findByText("Local history deleted.");
  expect(remove).toHaveBeenCalledWith({ includeTools: false });
  expect(await backend.listTools()).toHaveLength(1);
  expect((await backend.modelAccess()).apiKey.present).toBe(true);
  expect(
    (await backend.deletePreview()).items.every((i) => i.count === 0),
  ).toBe(true);
  await user.click(button("Delete local history…"));
  await user.click(
    screen.getByRole("checkbox", { name: "Also delete my tools" }),
  );
  await user.click(button("Delete history"));
  await screen.findByText("Local history deleted.");
  expect(remove).toHaveBeenLastCalledWith({ includeTools: true });
  await user.click(button("Tools"));
  expect(
    await screen.findByRole("button", { name: "Try a sample" }),
  ).toBeVisible();
});

it("exports and toggles Pause tool and Resume tool in the library", async () => {
  const backend = new MockBackend();
  const tool = await backend.installSampleTool();
  const user = userEvent.setup();
  render(<App backend={backend} />);
  await user.click(await screen.findByRole("button", { name: "Pause tool" }));
  expect(
    await screen.findByRole("button", { name: "Resume tool" }),
  ).toBeVisible();
  expect(button(`Run ${tool.name}…`)).toBeDisabled();
  await user.click(button("Export…"));
  expect(
    await screen.findByText(/Tool exported to \/Demo\/Exports/),
  ).toBeVisible();
  await user.click(button("Resume tool"));
  await waitFor(() => expect(button(`Run ${tool.name}…`)).toBeEnabled());
});

it("Try another sample opens the picker and displays failing row and column detail", async () => {
  const backend = new MockBackend();
  const bad = backend.importCsv(
    "bad-date.csv",
    sampleCsv.replace("01/05/26", "13/45/26"),
  );
  const pick = vi.spyOn(backend, "pickCsv").mockResolvedValue(bad);
  const check = vi.spyOn(backend, "buildTryInput");
  const user = userEvent.setup();
  render(<App backend={backend} />);
  await user.click(await screen.findByRole("button", { name: "Try a sample" }));
  await user.click(button("Review task summary"));
  await user.click(screen.getByRole("radio", { name: "March 4 (MM/DD/YY)" }));
  await user.click(button("Review what leaves this Mac"));
  await user.click(button("Build tool"));
  await waitFor(() => expect(button("Try another sample…")).toBeVisible(), {
    timeout: 4500,
  });
  await user.click(button("Try another sample…"));
  expect(pick).toHaveBeenCalledOnce();
  expect(check).toHaveBeenCalledWith(expect.any(String), bad.handle);
  expect(await screen.findByRole("alert")).toHaveTextContent(/row 1.*created/i);
  expect(
    screen.queryByRole("button", { name: "Save tool" }),
  ).not.toBeInTheDocument();
});

it("mock grant revisions refuse stale staged approvals even after resume", async () => {
  const backend = new MockBackend();
  const tool = await backend.installSampleTool();
  const input = await backend.loadSampleCsv();
  const folder = await backend.pickOutputDir();
  const staged = await backend.stageRun(
    tool.id,
    input.handle,
    folder,
    "fresh.csv",
  );
  if ("failure" in staged) throw new Error("stage failed");
  await backend.revokeTool(tool.id);
  expect(
    await backend.stageRun(tool.id, input.handle, folder, "paused.csv"),
  ).toMatchObject({ failure: { kind: "revoked" } });
  await backend.restoreTool(tool.id);
  expect(await backend.commitRun(staged.runId, staged.planHash)).toMatchObject({
    failure: { kind: "revoked" },
  });
});

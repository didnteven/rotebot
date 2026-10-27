import { act, render, screen, waitFor } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import { listen } from "@tauri-apps/api/event";
import { App } from "./App";
import { MockBackend } from "./backend/mock";

vi.mock("@tauri-apps/api/core", () => ({ isTauri: () => true }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));

it("tray navigation opens a tool Run screen or Create and releases listeners", async () => {
  const stop = vi.fn();
  vi.mocked(listen).mockResolvedValue(stop);
  const backend = new MockBackend();
  const tool = await backend.installSampleTool();
  const view = render(<App backend={backend} />);
  await screen.findByRole("button", { name: "Pause tool" });
  const run = vi
    .mocked(listen)
    .mock.calls.find(([name]) => name === "navigate://run")![1];
  const create = vi
    .mocked(listen)
    .mock.calls.find(([name]) => name === "navigate://create")![1];
  await act(async () =>
    run({ event: "navigate://run", id: 1, payload: tool.id }),
  );
  expect(
    await screen.findByRole("button", { name: "Stage and compare" }),
  ).toBeVisible();
  await act(async () =>
    create({ event: "navigate://create", id: 2, payload: null }),
  );
  expect(
    await screen.findByRole("button", { name: "Review task summary" }),
  ).toBeVisible();
  view.unmount();
  await waitFor(() => expect(stop).toHaveBeenCalledTimes(3));
});

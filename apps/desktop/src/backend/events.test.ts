import { expect, it, vi } from "vitest";
import { listen } from "@tauri-apps/api/event";
import { TauriBackend } from "./tauri";
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));
it("subscribes to the contract event names, unwraps payloads, and returns cleanup", async () => {
  const unlisten = vi.fn();
  vi.mocked(listen).mockResolvedValue(unlisten);
  const backend = new TauriBackend(),
    build = vi.fn(),
    show = vi.fn();
  expect(await backend.onBuildEvent("build-1", build)).toBe(unlisten);
  expect(listen).toHaveBeenNthCalledWith(
    1,
    "build://build-1",
    expect.any(Function),
  );
  const event = {
    event: "build://build-1",
    id: 1,
    payload: { phase: "checking" },
  };
  vi.mocked(listen).mock.calls[0][1](event);
  expect(build).toHaveBeenCalledWith(event.payload);
  expect(await backend.onShowEvent(show)).toBe(unlisten);
  expect(listen).toHaveBeenNthCalledWith(
    2,
    "show://state",
    expect.any(Function),
  );
  vi.mocked(listen).mock.calls[1][1]({
    event: "show://state",
    id: 2,
    payload: { kind: "finished" },
  });
  expect(show).toHaveBeenCalledWith({ kind: "finished" });
});

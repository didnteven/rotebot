import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it, vi } from "vitest";
import { App } from "./App";
import { MockBackend } from "./backend/mock";
import type { AppInfo } from "./backend/types";

it.each<[AppInfo["secretStore"], boolean, boolean]>([
  ["file", true, true],
  ["keychain", true, false],
  ["file", false, false],
  ["keychain", false, false],
  ["memory", false, false],
])(
  "Settings reflects %s storage with debugBuild=%s",
  async (secretStore, debugBuild, visible) => {
    const backend = new MockBackend();
    const info = await backend.appInfo();
    vi.spyOn(backend, "appInfo").mockResolvedValue({
      ...info,
      platform: "macos",
      mock: false,
      secretStore,
      debugBuild,
    });
    const user = userEvent.setup();
    render(<App backend={backend} />);
    await screen.findByRole("button", { name: "Try a sample" });
    await user.click(screen.getByRole("button", { name: "Settings" }));
    const note = screen.queryByText(
      "Development build: secrets are stored in a private file in Rotebot's data folder instead of the Keychain.",
    );
    if (visible) expect(note).toBeVisible();
    else expect(note).not.toBeInTheDocument();
  },
);

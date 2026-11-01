import { act, fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { BotAvatar, BotStatus, botHash, botIdentity, saveBotIdentity, ThemePicker } from "./Bot";
import { useAction } from "./ui";
import { withoutBotArtwork } from "./test/trustedArtwork";

describe("task bots", () => {
  it("keeps a draft identity after save and falls back safely for legacy tools", () => {
    expect(botIdentity("legacy-tool")).toBe("legacy-tool");
    saveBotIdentity("saved-tool", "draft-portrait");
    expect(botIdentity("saved-tool")).toBe("draft-portrait");
    expect(botHash(botIdentity("saved-tool"))).toBe(botHash("draft-portrait"));
  });
  it("uses inert local artwork even when a task identity contains markup", () => {
    const { container } = render(<BotAvatar seed={'<svg onload="alert(1)">'} state="working" />);
    expect(withoutBotArtwork(container).children).toHaveLength(0);
    expect(container.querySelector(".bot-working")).toHaveAttribute("aria-hidden", "true");
  });
  it("switches expression artwork while preserving the task character", () => {
    const { container, rerender } = render(<BotAvatar seed="task-portrait" state="idle" />);
    const family = [...container.querySelector(".bot-sprite")!.classList].find(name => name.startsWith("bot-family-"));
    for (const [state, pose] of [["working", 1], ["building", 1], ["waiting", 2], ["success", 3], ["error", 4], ["paused", 5]] as const) {
      rerender(<BotAvatar seed="task-portrait" state={state} />);
      expect(container.querySelector(".bot-sprite")).toHaveClass(family!, `bot-pose-${pose}`);
    }
  });
  it("announces progress as text separately from decorative artwork", () => {
    render(<BotStatus label="Checking your tool…" state="waiting" />);
    expect(screen.getByRole("status")).toHaveTextContent("Checking your tool…");
  });
  it("applies and remembers an explicit theme", () => {
    render(<ThemePicker />);
    fireEvent.change(screen.getByLabelText("Appearance"), { target: { value: "dark" } });
    expect(document.documentElement.dataset.theme).toBe("dark");
    expect(localStorage.getItem("macroscope.theme")).toBe("dark");
    fireEvent.change(screen.getByLabelText("Appearance"), { target: { value: "system" } });
  });
  it("prevents duplicate submissions before React commits the busy state", async () => {
    let finish!: () => void;
    let calls = 0;
    const job = () => { calls++; return new Promise<void>(resolve => { finish = resolve; }); };
    function Harness() {
      const { act: run, busy } = useAction();
      return <button onClick={() => { void run(job); void run(job); }}>{busy ? "Saving" : "Save"}</button>;
    }
    render(<Harness />);
    fireEvent.click(screen.getByText("Save"));
    expect(calls).toBe(1);
    await act(async () => finish());
    expect(screen.getByText("Save")).toBeVisible();
  });
});

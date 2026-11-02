import { act, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { BuildProgress } from "./BuildProgress";
import type { BuildPhase, BuildState } from "./backend/types";

function build(phase: BuildPhase, history: BuildPhase[]): BuildState {
  return {
    buildId: "build-1", draftId: "draft-1", route: "offline_demo", phase,
    events: history.map((eventPhase, index) => ({ buildId: "build-1", seq: index, phase: eventPhase, message: `Host update ${index}`, atMs: 1000 + index * 2000 })),
    candidate: null, checks: [], comparison: null, spentMicros: null, failure: null, routeLabel: "Offline demo",
  };
}
function view(state: BuildState, stop = vi.fn()) {
  return <BuildProgress build={state} seed="test-bot" name="Daily report" busy={false} stop={stop} />;
}
function stages() { return within(screen.getByRole("list", { name: "Build stages" })).getAllByRole("listitem"); }
afterEach(() => vi.useRealTimers());

describe("build workshop", () => {
  it("tracks retries without keeping later stages marked as complete", () => {
    const { rerender } = render(view(build("checking", ["queued", "writing", "candidate", "checking"])));
    expect(stages()[1]).toHaveTextContent("Build: complete");
    expect(stages()[2]).toHaveAttribute("aria-current", "step");
    rerender(view(build("writing", ["queued", "writing", "candidate", "checking", "writing"])));
    expect(stages()[1]).toHaveTextContent("Build: in progress");
    expect(stages()[2]).toHaveTextContent("Test: pending");
    expect(screen.getByRole("status")).toHaveTextContent("Building your tool");
  });

  it("stops the clock at the terminal event and marks the interrupted stage", () => {
    vi.useFakeTimers();
    vi.setSystemTime(8000);
    const { rerender } = render(view(build("checking", ["queued", "writing", "checking"])));
    expect(screen.getByText("Elapsed · 7s")).toBeVisible();
    act(() => vi.advanceTimersByTime(2000));
    expect(screen.getByText("Elapsed · 9s")).toBeVisible();
    rerender(view(build("failed", ["queued", "writing", "checking", "failed"])));
    expect(stages()[2]).toHaveTextContent("Test: failed");
    expect(stages()[3]).toHaveTextContent("Review: pending");
    expect(screen.getByText("Duration · 6s")).toBeVisible();
    act(() => vi.advanceTimersByTime(5000));
    expect(screen.getByText("Duration · 6s")).toBeVisible();
    expect(screen.queryByRole("button", { name: "Stop build" })).not.toBeInTheDocument();
  });

  it("leaves review awaiting the user after successful checks", () => {
    render(view(build("ready", ["queued", "writing", "checking", "ready"])));
    expect(stages()[2]).toHaveTextContent("Test: complete");
    expect(stages()[3]).toHaveTextContent("Review: ready for your review");
    expect(stages()[3]).not.toHaveTextContent("complete");
    expect(screen.getByRole("status")).toHaveTextContent("Check the result below before saving");
  });

  it("does not invent timing or completed stages when no events are available", () => {
    render(view(build("candidate", [])));
    expect(screen.queryByText(/Elapsed|Duration/)).not.toBeInTheDocument();
    expect(stages()[0]).toHaveTextContent("Queue: pending");
    expect(stages()[1]).toHaveTextContent("Build: in progress");
    expect(screen.getByRole("status")).toHaveTextContent("Preparing the checks");
  });

  it("keeps the stop action and real event details accessible", async () => {
    const user = userEvent.setup();
    const stop = vi.fn();
    render(view(build("writing", ["queued", "writing"]), stop));
    await user.click(screen.getByRole("button", { name: "Stop build" }));
    expect(stop).toHaveBeenCalledOnce();
    await user.click(screen.getByText("Latest update"));
    expect(screen.getByText("Host update 1")).toBeVisible();
  });
});

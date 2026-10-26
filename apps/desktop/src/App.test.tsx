import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { App } from "./App";
import { MockBackend } from "./backend/mock";

describe("desktop shell", () => {
  it("opens Tools with the sample and describe actions", async () => {
    render(<App backend={new MockBackend()} />);
    expect(
      screen.getByRole("heading", { name: "Tools", level: 1 }),
    ).toBeVisible();
    expect(
      await screen.findByRole("button", { name: "Try a sample" }),
    ).toBeVisible();
    expect(
      screen.getByRole("button", { name: "Describe a CSV task" }),
    ).toBeVisible();
    expect(screen.getByRole("button", { name: "Tools" })).toHaveAttribute(
      "aria-current",
      "page",
    );
  });

  it("navigates every destination with the keyboard", async () => {
    const user = userEvent.setup();
    render(<App backend={new MockBackend()} />);
    await screen.findByRole("button", { name: "Try a sample" });
    await user.tab();
    expect(screen.getByRole("button", { name: "Tools" })).toHaveFocus();
    for (const destination of [
      "Activity",
      "Connections",
      "Privacy",
      "Settings",
    ]) {
      await user.tab();
      await user.keyboard("{Enter}");
      expect(
        screen.getByRole("heading", { name: destination, level: 1 }),
      ).toBeVisible();
      expect(screen.getByRole("button", { name: destination })).toHaveAttribute(
        "aria-current",
        "page",
      );
    }
    await user.click(screen.getByRole("button", { name: "Tools" }));
    expect(screen.getByRole("button", { name: "Try a sample" })).toBeVisible();
  });

  it("renders backend tools and treats tool names as text", async () => {
    render(
      <App
        backend={
          new MockBackend([
            { id: "tool-1", name: "<script>fabricated</script>", version: 2 },
          ])
        }
      />,
    );
    expect(
      await screen.findByText("<script>fabricated</script>"),
    ).toBeVisible();
    expect(
      screen.queryByRole("button", { name: "Try a sample" }),
    ).not.toBeInTheDocument();
    expect(document.querySelector("main script")).toBeNull();
  });

  it("shows a scrubbed failure instead of claiming the library is empty", async () => {
    const backend = new MockBackend();
    vi.spyOn(backend, "listTools").mockRejectedValue(
      new Error("private detail"),
    );
    render(<App backend={backend} />);
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Tools could not be loaded",
    );
    expect(screen.queryByText("private detail")).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "Try a sample" }),
    ).not.toBeInTheDocument();
  });
});

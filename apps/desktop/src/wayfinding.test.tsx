import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it, vi } from "vitest";
import { StrictMode } from "react";
import { App } from "./App";
import { MockBackend } from "./backend/mock";
import { RunFlow } from "./RunFlow";
import { Privacy } from "./History";
import { ScheduleSection } from "./Schedule";
import type { Schedule } from "./backend/types";

it("returns from Settings to an unfinished task without losing its description", async () => {
  const user = userEvent.setup();
  render(<App backend={new MockBackend()} />);
  const create = await screen.findByRole("button", { name: "Describe a task" });
  await waitFor(() => expect(create).toBeEnabled());
  await user.click(create);
  await user.type(screen.getByLabelText("What should the tool do?"), "Summarize my calendar");
  await user.click(screen.getByRole("button", { name: "Settings" }));
  expect(screen.getByRole("group", { name: "Which model builds your tools" })).toHaveFocus();
  await user.click(screen.getByRole("button", { name: "My tools" }));
  expect(screen.getByLabelText("What should the tool do?")).toHaveValue("Summarize my calendar");
  await user.click(screen.getByRole("button", { name: "Back" }));
  expect(screen.getByRole("button", { name: "Describe a task" })).toBeVisible();
});

it("opens a saved schedule with its actual frequency, time, and destinations", async () => {
  const backend = new MockBackend();
  const tool = await backend.installSampleTool();
  const schedule: Schedule = { id: "existing", toolId: tool.id, frequency: { kind: "weekly", time: "16:30", weekday: 5 }, outputs: { today: false, notification: true, folder: null, filename: "weekly-{date}.txt" }, status: "enabled", nextRunMs: null, lastDueMs: 0, approvedAtMs: Date.now(), versionHash: tool.versionHash, binding: "binding", folderIdentity: null };
  vi.spyOn(backend, "listSchedules").mockResolvedValue([schedule]);
  render(<ScheduleSection backend={backend} tool={tool} />);
  await waitFor(() => expect(screen.getByLabelText("Frequency")).toHaveValue("weekly"));
  expect(screen.getByLabelText("Time")).toHaveValue("16:30");
  expect(screen.getByLabelText("Weekday")).toHaveValue("5");
  expect(screen.getByLabelText("Add to Today")).not.toBeChecked();
  expect(screen.getByLabelText("I approve this running unattended")).not.toBeChecked();
  fireEvent.change(screen.getByLabelText("Frequency"), { target: { value: "monthly" } });
  fireEvent.change(screen.getByLabelText("Day"), { target: { value: "0" } });
  fireEvent.click(screen.getByLabelText("I approve this running unattended"));
  expect(screen.getByRole("button", { name: "Update schedule" })).toBeDisabled();
  expect(screen.getByText("Choose a day from 1 to 31.")).toBeVisible();
});

it("recovers from a failed initial tool load and works in Strict Mode", async () => {
  const backend = new MockBackend();
  const tool = await backend.installSampleTool();
  const get = vi.spyOn(backend, "getTool").mockRejectedValueOnce(new Error("Temporary connection problem"));
  const view = render(<RunFlow backend={backend} tool={tool} close={() => {}} changed={async () => {}} />);
  await screen.findByRole("alert");
  await userEvent.click(screen.getByRole("button", { name: "Try again" }));
  await waitFor(() => expect(screen.getByRole("button", { name: "Choose input file" })).toBeEnabled());
  expect(get).toHaveBeenCalledTimes(2);
  view.unmount();
  render(<StrictMode><RunFlow backend={backend} tool={tool} close={() => {}} changed={async () => {}} /></StrictMode>);
  await waitFor(() => expect(screen.getByRole("button", { name: "Choose input file" })).toBeEnabled());
});

it("allows Emergency stop while privacy history is still loading", async () => {
  const backend = new MockBackend();
  const overview = await backend.privacyOverview();
  let finish!: (value: typeof overview) => void;
  vi.spyOn(backend, "privacyOverview").mockImplementationOnce(() => new Promise(resolve => { finish = resolve; }));
  const stop = vi.spyOn(backend, "emergencyStop");
  render(<Privacy backend={backend} status={await backend.modelAccess()} update={() => {}} stopped={async () => {}} />);
  const button = screen.getByRole("button", { name: "Emergency stop" });
  expect(button).toBeEnabled();
  await userEvent.click(button);
  await waitFor(() => expect(stop).toHaveBeenCalledOnce());
  await act(async () => finish(overview));
});

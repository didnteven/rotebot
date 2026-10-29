import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it, vi } from "vitest";
import { ShowPanel } from "./Show";
import { MockBackend } from "./backend/mock";

const button = (name: string) => screen.getByRole("button", { name });
async function open(backend = new MockBackend()) {
  const user = userEvent.setup();
  const close = vi.fn();
  const analyze = vi.spyOn(backend, "consentAndAnalyze");
  const discard = vi.spyOn(backend, "discardRecording");
  const approve = vi.spyOn(backend, "approveRecordingDraft");
  const create = vi.spyOn(backend, "draftTaskNoInput");
  const build = vi.spyOn(backend, "startBuild");
  const view = render(<ShowPanel backend={backend} state={await backend.showState()} update={vi.fn()} close={close}
    createFlow={{ status: await backend.modelAccess(), connectors: [], openSettings: vi.fn(), connect: vi.fn(), close, saved: vi.fn(async () => {}) }} />);
  await screen.findByText("Screen recording isn't set up on this Mac yet");
  return { ...view, backend, user, analyze, discard, approve, create, build, close };
}
async function review(ctx: Awaited<ReturnType<typeof open>>) {
  await ctx.user.click(button("Try it with a sample recording"));
  expect(screen.getByRole("heading", { name: "Recording" })).toHaveFocus();
  expect(screen.getByRole("timer")).toHaveTextContent("0:00");
  expect(screen.getByText("Frames kept: 3, hidden for privacy: 3")).toBeVisible();
  await ctx.user.click(button("Stop"));
  expect(screen.getByRole("heading", { name: "Check what will be sent" })).toHaveFocus();
}

it("walks all five screens, consents only on Send, then builds, previews and saves through CreateFlow", async () => {
  const backend = new MockBackend();
  await backend.saveApiKey("synthetic-test-key");
  const ctx = await open(backend);
  expect(screen.getByRole("heading", { name: "Show Macroscope what to do" })).toHaveFocus();
  expect(screen.getByText("Advanced: app activity log").closest("details")).not.toHaveAttribute("open");
  expect(button("Discard recording")).toBeVisible();
  await review(ctx);
  expect(ctx.analyze).not.toHaveBeenCalled();
  expect((await backend.privacyOverview()).outbound).toEqual([]);
  expect(screen.getAllByRole("img")).toHaveLength(3);
  expect(screen.getAllByRole("img")[0]).toHaveAccessibleName(/private content hidden in black/);
  const text = screen.getByLabelText("Cleared recording text").textContent;
  expect(text).toContain('"window_title":null');
  expect(screen.getByText("Nothing leaves your Mac until you press Send.")).toBeVisible();
  await ctx.user.click(button("Send for analysis"));
  expect(ctx.analyze).toHaveBeenCalledExactlyOnceWith(expect.any(String), expect.stringContaining("mock-consent-"));
  const rows = (await backend.privacyOverview()).outbound;
  expect(rows).toHaveLength(1);
  expect(rows[0].purpose).toBe("Recording analysis");
  expect(JSON.parse(rows[0].payloadText!).untrusted_data_json).toBe(text);
  expect(screen.getByRole("heading", { name: "What we understood" })).toHaveFocus();
  expect(screen.getByRole("heading", { name: "You'll still need to do this part yourself" })).toBeVisible();
  const questions = screen.getAllByRole("group");
  expect(questions).toHaveLength(3);
  for (const q of questions) {
    expect(within(q).getAllByRole("radio").filter(r => (r as HTMLInputElement).checked)).toHaveLength(1);
    expect(within(q).getByLabelText("Any is fine")).toBeVisible();
  }
  await ctx.user.click(within(questions[0]).getByLabelText("Any is fine"));
  expect(ctx.approve).not.toHaveBeenCalled();
  await ctx.user.click(button("Looks right"));
  await waitFor(() => expect(screen.getByRole("heading", { name: "Create the tool" })).toHaveFocus());
  expect(screen.getByRole("heading", { name: "Check the plan" })).toBeVisible();
  expect(ctx.create).toHaveBeenCalledOnce();
  expect(ctx.create.mock.calls[0][0]).toContain("Manual only (unsupported)");
  expect(ctx.build).not.toHaveBeenCalled();
  expect(button("Send and build")).toBeEnabled();
  expect(screen.queryByRole("button", { name: "Discard recording" })).not.toBeInTheDocument();
  await ctx.user.click(button("Send and build"));
  expect(ctx.build).toHaveBeenCalledExactlyOnceWith(expect.any(String), {outboundReviewed:true});
  await screen.findByRole("heading", {name:"Result"}, {timeout:4000});
  expect(await screen.findByLabelText("Answer", {}, {timeout:4000})).toBeVisible();
  await ctx.user.click(button("Yes, save this tool"));
  await waitFor(async () => expect(await backend.listTools()).toHaveLength(1));
}, 10000);

it.each([0, 1, 2, 3])("declining on screen %i discards without creating a task", async step => {
  const ctx = await open();
  if (step === 1) await ctx.user.click(button("Try it with a sample recording"));
  if (step >= 2) await review(ctx);
  if (step === 3) await ctx.user.click(button("Send for analysis"));
  await ctx.user.click(button("Discard recording"));
  expect(screen.getByRole("heading", { name: "Show Macroscope what to do" })).toHaveFocus();
  if (step > 0) {
    expect(ctx.discard).toHaveBeenCalledOnce();
    await expect(ctx.backend.listRecordingFrames(ctx.discard.mock.calls[0][0])).rejects.toThrow(/discarded/);
  }
  expect(ctx.approve).not.toHaveBeenCalled();
  expect(ctx.create).not.toHaveBeenCalled();
  expect(ctx.analyze).toHaveBeenCalledTimes(step === 3 ? 1 : 0);
});

it("shows a friendly Local-only error and permits discard", async () => {
  const backend = new MockBackend();
  await backend.setLocalOnly(true);
  const ctx = await open(backend);
  await review(ctx);
  await ctx.user.click(button("Send for analysis"));
  expect(screen.getByRole("alert")).toHaveTextContent("Local only is on");
  expect(ctx.approve).not.toHaveBeenCalled();
  expect((await backend.privacyOverview()).outbound).toEqual([]);
  await ctx.user.click(button("Discard recording"));
  expect(ctx.discard).toHaveBeenCalledOnce();
});

it("shows a loading state and disables repeat sends while analysis is pending", async () => {
  const backend = new MockBackend();
  const original = backend.consentAndAnalyze.bind(backend);
  const ctx = await open(backend);
  await review(ctx);
  let release!: () => void;
  const gate = new Promise<void>(resolve => { release = resolve; });
  ctx.analyze.mockImplementation(async (...args) => { await gate; return original(...args); });
  await ctx.user.click(button("Send for analysis"));
  expect(screen.getByRole("status")).toHaveTextContent("Analyzing your recording");
  expect(button("Send for analysis")).toBeDisabled();
  release();
  await screen.findByRole("heading", { name: "What we understood" });
});

it("shows an empty review and never sends it", async () => {
  const backend = new MockBackend();
  const original = backend.previewRecording.bind(backend);
  vi.spyOn(backend, "previewRecording").mockImplementation(async id => ({ ...await original(id), frames: [] }));
  const ctx = await open(backend);
  await review(ctx);
  expect(screen.getByText(/No frames were safe to keep/)).toBeVisible();
  expect(button("Send for analysis")).toBeDisabled();
  expect(ctx.analyze).not.toHaveBeenCalled();
});

it("discards the memory session when the panel closes", async () => {
  const ctx = await open();
  await ctx.user.click(button("Try it with a sample recording"));
  ctx.unmount();
  expect(ctx.discard).toHaveBeenCalledOnce();
});

it("requires valid answers before approval, rejects stale consent, and expires idle sessions", async () => {
  const backend = new MockBackend();
  const id = await backend.startRecording("sample");
  await expect(backend.previewRecording(id)).rejects.toThrow(/Stop/);
  await backend.stopRecording(id);
  const p = await backend.previewRecording(id);
  await expect(backend.consentAndAnalyze(id, "wrong")).rejects.toThrow(/consent/);
  const d = await backend.consentAndAnalyze(id, p.payloadHash);
  await expect(backend.approveRecordingDraft(id)).rejects.toThrow(/Answer every question/);
  await expect(backend.answerRecordingQuestions(id, {})).rejects.toThrow(/Answer every question/);
  const answers = Object.fromEntries(d.questions.map(q => [q.id, q.default]));
  await backend.answerRecordingQuestions(id, answers);
  await expect(backend.answerRecordingQuestions(id, { ...answers, extra: "any" })).rejects.toThrow();
  await expect(backend.approveRecordingDraft(id)).rejects.toThrow(/Answer every question/);
  vi.spyOn(Date, "now").mockReturnValue(Date.now() + 30 * 60 * 1000);
  await expect(backend.listRecordingFrames(id)).rejects.toThrow(/expired/);
});

it("cannot continue with an unanswered question even if a backend returns a missing default", async () => {
  const backend = new MockBackend();
  const analyze = backend.consentAndAnalyze.bind(backend);
  backend.consentAndAnalyze = async (...args) => {
    const d = await analyze(...args);
    d.questions[0].default = "";
    return d;
  };
  const ctx = await open(backend);
  await review(ctx);
  await ctx.user.click(button("Send for analysis"));
  expect(button("Looks right")).toBeDisabled();
  expect(ctx.approve).not.toHaveBeenCalled();
  await ctx.user.click(within(screen.getAllByRole("group")[0]).getByLabelText("Any is fine"));
  expect(button("Looks right")).toBeEnabled();
});

it("invalidates reviewed consent across a Local-only policy change", async () => {
  const b = new MockBackend();
  const id = await b.startRecording("sample");
  await b.stopRecording(id);
  const p = await b.previewRecording(id);
  await b.setLocalOnly(true);
  await b.setLocalOnly(false);
  await expect(b.consentAndAnalyze(id, p.payloadHash)).rejects.toThrow(/consent/);
});

it("applies the existing custom endpoint cap before analysis and consumes attempted consent", async () => {
  const b = new MockBackend();
  await b.saveCustomEndpoint("https://sample.invalid/v1", "sample-image-model");
  await b.saveCustomEndpointToken();
  await b.testCustomEndpoint();
  await b.selectRoute("custom_endpoint");
  await b.setCustomEndpointOptions(null, 1); // The endpoint test used the one call.
  const id = await b.startRecording("sample");
  await b.stopRecording(id);
  const p = await b.previewRecording(id);
  await expect(b.consentAndAnalyze(id, p.payloadHash)).rejects.toThrow(/call cap/);
  await b.setCustomEndpointOptions(null, 2);
  await expect(b.consentAndAnalyze(id, p.payloadHash)).rejects.toThrow(/consent/);
  expect((await b.privacyOverview()).outbound.filter(r => r.purpose === "Recording analysis")).toEqual([]);
});

it("requests permissions only on the explicit button, then starts the window source and waits for review consent", async () => {
  const backend = new MockBackend();
  let granted = false;
  vi.spyOn(backend, "recordingStatus").mockImplementation(async () => ({
    captureAvailable: granted, helperAvailable: true, screenRecordingGranted: granted,
    accessibilityGranted: granted, pickerState: granted ? "idle" : "permission_required",
    appName: null, reason: null,
  }));
  const permission = vi.spyOn(backend, "requestRecordingPermission").mockImplementation(async () => {
    granted = true;
    return backend.recordingStatus();
  });
  // Fabricated backend data only; the test verifies which native command the UI requests.
  const originalStart = backend.startRecording.bind(backend);
  const start = vi.spyOn(backend, "startRecording").mockImplementation(source => originalStart(source === "window" ? "sample" : source));
  const originalPreview = backend.previewRecording.bind(backend);
  vi.spyOn(backend, "previewRecording").mockImplementation(async id => ({...await originalPreview(id), sample:false}));
  const ctx = await open(backend);
  expect(permission).not.toHaveBeenCalled();
  expect(screen.getByText(/Open System Settings > Privacy & Security > Screen Recording/)).toBeVisible();
  expect(button("Try it with a sample recording")).toBeVisible();
  await ctx.user.click(button("Try again"));
  expect(permission).not.toHaveBeenCalled();
  await ctx.user.click(button("Allow recording access"));
  expect(permission).toHaveBeenCalledOnce();
  await ctx.user.click(await screen.findByRole("button", {name:"Choose a window to record"}));
  expect(start).toHaveBeenCalledExactlyOnceWith("window");
  expect(ctx.analyze).not.toHaveBeenCalled();
  await ctx.user.click(button("Stop"));
  expect(await screen.findByRole("heading", {name:"Check what will be sent"})).toBeVisible();
  expect(ctx.analyze).not.toHaveBeenCalled();
  expect(button("Send for analysis")).toBeVisible();
});

import { afterEach, expect, it, vi } from "vitest";
import { MockBackend } from "./mock";
import { sampleCsv } from "./sample";

afterEach(() => vi.useRealTimers());
async function draft(backend: MockBackend) {
  const input = await backend.loadSampleCsv();
  const task = await backend.draftTask(
    input.handle,
    "Orders from Gmail, Slack, Notion, Google Calendar, Jira, Linear, GitHub; no slackers.",
  );
  return backend.answerQuestion(task.draftId, "date-format", "mm_dd_yy");
}
it("loads the complete 18-row fabricated fixture and detects connectors by word", async () => {
  const backend = new MockBackend(),
    input = await backend.loadSampleCsv();
  expect(input.rowCount).toBe(18);
  expect(input.rows).toHaveLength(8);
  expect(input.columns).toEqual(["order_id", "created", "amount", "customer"]);
  expect(input.byteSize).toBe(new TextEncoder().encode(sampleCsv).length);
  const d = await backend.draftTask(
    input.handle,
    "gmail, slackers, githubbing, google calendar and Notion",
  );
  expect(d.readyToBuild).toBe(false);
  expect(d.mentionedConnectors).toEqual(["gmail", "google_calendar", "notion"]);
  expect(d.questions[0].prompt).toBe("Does 03/04/26 mean March 4 or April 3?");
});
it("requires cloud consent, emits all milestones, and independently checks the sample", async () => {
  vi.useFakeTimers();
  const backend = new MockBackend();
  await backend.saveApiKey("demo-key");
  const d = await draft(backend);
  await expect(
    backend.startBuild(d.draftId, { outboundReviewed: false }),
  ).rejects.toThrow("Review");
  const state = await backend.startBuild(d.draftId, { outboundReviewed: true });
  const phases: string[] = [];
  const off = await backend.onBuildEvent(state.buildId, (s) =>
    phases.push(s.phase),
  );
  await vi.advanceTimersByTimeAsync(2000);
  expect(phases).toEqual(["writing", "candidate", "checking", "ready"]);
  const ready = await backend.buildState(state.buildId);
  expect(ready.events.map((e) => e.phase)).toEqual([
    "queued",
    "writing",
    "candidate",
    "checking",
    "ready",
  ]);
  expect(ready.checks).toHaveLength(5);
  expect(ready.checks.every((c) => c.passed)).toBe(true);
  expect(ready.comparison).toMatchObject({
    rowsIn: 18,
    rowsOut: 18,
    outputRows: [
      ["ORD-001", "2026-01-05", "1240.00"],
      ...ready.comparison!.outputRows.slice(1),
    ],
  });
  expect(ready.comparison!.outputRows).toHaveLength(8);
  off();
});
it("falls back without model access and does not send real data in the outbound payload", async () => {
  vi.useFakeTimers();
  const backend = new MockBackend(),
    d = await draft(backend);
  expect(await backend.modelAccess()).toMatchObject({
    selectedRoute: null,
    effectiveRoute: "offline_demo",
    apiKey: { present: false },
    muse: { kind: "not_signed_in" },
  });
  const build = await backend.startBuild(d.draftId, {
    outboundReviewed: false,
  });
  expect(build).toMatchObject({
    phase: "queued",
    route: "offline_demo",
    failure: null,
  });
  await vi.advanceTimersByTimeAsync(2000);
  expect(await backend.buildState(build.buildId)).toMatchObject({
    phase: "ready",
    failure: null,
  });
  expect((await backend.privacyOverview()).outbound).toEqual([]);
  await backend.saveApiKey("secret-abcd");
  const preview = await backend.outboundPreview(d.draftId);
  expect(preview.route).toBe("api_key");
  expect(preview.payloadText).toContain("SYN-001");
  expect(preview.payloadText).not.toContain("Ada Example");
  expect(preview.payloadText).not.toContain("ORD-001");
  expect(preview.payloadText).not.toContain("secret-abcd");
});
it("does not emit later build events after stop or local-only revocation", async () => {
  vi.useFakeTimers();
  const backend = new MockBackend();
  await backend.saveApiKey("demo-key");
  const d = await draft(backend);
  const state = await backend.startBuild(d.draftId, { outboundReviewed: true });
  const stop = backend.setLocalOnly(true);
  await vi.advanceTimersByTimeAsync(2000);
  await stop;
  expect(await backend.buildState(state.buildId)).toMatchObject({
    phase: "stopped",
    checks: [],
  });
  expect((await backend.modelAccess()).egress).toBe("local_only");
});
it("hashes all staged rows, refuses collisions and forged hashes, and honors cancel", async () => {
  const backend = new MockBackend(),
    tool = await backend.installSampleTool(),
    folder = await backend.pickOutputDir();
  const a = await backend.loadSampleCsv();
  const b = backend.importCsv(
    "second.csv",
    sampleCsv.replace("ORD-018", "ORD-999"),
  );
  const stageA = await backend.stageRun(tool.id, a.handle, folder, "out.csv");
  const stageB = await backend.stageRun(tool.id, b.handle, folder, "other.csv");
  if ("failure" in stageA || "failure" in stageB)
    throw new Error("Staging unexpectedly failed");
  expect(await backend.commitRun(stageA.runId, "forged")).toMatchObject({
    failure: { kind: "input_changed" },
  });
  const receiptA = await backend.commitRun(stageA.runId, stageA.planHash),
    receiptB = await backend.commitRun(stageB.runId, stageB.planHash);
  if ("failure" in receiptA || "failure" in receiptB)
    throw new Error("Commit unexpectedly failed");
  expect(receiptA.committedDigest).not.toBe(receiptB.committedDigest);
  const collision = await backend.stageRun(
    tool.id,
    a.handle,
    folder,
    "out.csv",
  );
  if ("failure" in collision) throw new Error("Stage unexpectedly failed");
  expect(collision.destinationExists).toBe(true);
  expect(
    await backend.commitRun(collision.runId, collision.planHash),
  ).toMatchObject({ failure: { kind: "destination_exists" } });
  await backend.discardRun(collision.runId);
  expect(
    await backend.commitRun(collision.runId, collision.planHash),
  ).toMatchObject({ failure: { kind: "revoked" } });
});
it("refuses insecure endpoints and clears authorization when endpoints change", async () => {
  vi.useFakeTimers();
  const backend = new MockBackend();
  await expect(
    backend.setConnectorUrl("notion", "http://example.com"),
  ).rejects.toThrow("HTTPS");
  await expect(
    backend.setConnectorUrl("notion", "https://user:secret@example.com"),
  ).rejects.toThrow("credentials");
  await backend.setClientCredentials("gmail", "id", "secret");
  const pending = await backend.beginConnect("gmail");
  await backend.setConnectorUrl("gmail", "https://example.com/new-mcp");
  const completed = expect(
    backend.completeConnect(pending.pendingId),
  ).rejects.toThrow("canceled");
  await vi.advanceTimersByTimeAsync(1000);
  await completed;
  expect(
    (await backend.listConnectors()).find((c) => c.id === "gmail"),
  ).toMatchObject({
    verification: "unverified",
    hasClientCredentials: false,
    state: { kind: "not_connected" },
  });
});
it("remembers dismissals and stops recording at the five-minute limit", async () => {
  vi.useFakeTimers();
  const backend = new MockBackend();
  await backend.startShow();
  await vi.advanceTimersByTimeAsync(300000);
  expect(await backend.showState()).toMatchObject({
    kind: "finished",
    durationSeconds: 300,
    appsSeen: ["Google Chrome", "Numbers"],
  });
  expect(await backend.connectionSuggestions()).toHaveLength(1);
  await backend.dismissSuggestion("gmail");
  await backend.startShow();
  await backend.stopShow();
  expect(await backend.connectionSuggestions()).toEqual([]);
});

it("saves the immutable build revision after the editable draft changes", async () => {
  vi.useFakeTimers();
  const backend = new MockBackend();
  await backend.selectRoute("offline_demo");
  const task = await draft(backend);
  const build = await backend.startBuild(task.draftId, {
    outboundReviewed: true,
  });
  const changed = structuredClone(task.mapping);
  changed.output_columns[0].name = "Changed after build started";
  await backend.updateMapping(task.draftId, changed, "different.csv");
  await vi.advanceTimersByTimeAsync(2000);
  const saved = await backend.saveTool(build.buildId, "Frozen build");
  expect(saved.mapping.output_columns[0].name).toBe("Order");
  expect(saved.outputColumns).toEqual(["Order", "Date", "Amount"]);
});

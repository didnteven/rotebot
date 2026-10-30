import { expect, it } from "vitest";
import { neededConnectors } from "./Show";
import type { ConnectorInfo, TaskDraftV2 } from "./backend/types";

const info = (id: string, name: string, connected: boolean) =>
  ({ id, name, state: connected ? { kind: "connected", sinceMs: 1, toolCount: 3 } : { kind: "not_connected" } }) as unknown as ConnectorInfo;
const step = (id: string, app: string, action: string, capability: TaskDraftV2["steps"][number]["capability"]) =>
  ({ id, order: 1, app, action, capability, evidence_frames: [1] }) as TaskDraftV2["steps"][number];
const catalog = [info("gmail", "Gmail", false), info("jira", "Jira (Atlassian)", true), info("custom", "Custom MCP server", false)];

it("matches connection steps to catalog services by name, once each", () => {
  const found = neededConnectors(
    [step("a", "Gmail", "Read the latest messages", "connection"), step("b", "Gmail", "Read ChatGPT email", "connection"), step("c", "Jira", "List open issues", "connection")],
    catalog,
  );
  expect(found.connectors.map(c => c.id)).toEqual(["gmail", "jira"]);
  expect(found.other).toBe(false);
});

it("ignores steps that are not connections and flags unknown services", () => {
  const found = neededConnectors(
    [step("a", "Gmail", "Open the inbox", "manual"), step("b", "Airtable", "Read the base", "connection")],
    catalog,
  );
  expect(found.connectors).toEqual([]);
  expect(found.other).toBe(true);
});

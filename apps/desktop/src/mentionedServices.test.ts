import { expect, it } from "vitest";
import { connectOffers, readyConnectors } from "./mentionedServices";
import type { ConnectorInfo } from "./backend/types";

const info = (id: string, service: string | undefined, connected: boolean) =>
  ({ id, name: id, service, state: connected ? { kind: "connected", sinceMs: 1, toolCount: 3 } : { kind: "not_connected" } }) as unknown as ConnectorInfo;
const list = [info("mail-gmail", "gmail", true), info("gmail", "gmail", false), info("slack", undefined, false)];

it("switches on the connected connection of a mentioned service, whichever way it signs in", () => {
  expect(readyConnectors(["gmail"], list, [])).toEqual(["mail-gmail"]);
  expect(readyConnectors(["mail-gmail", "gmail"], list, [])).toEqual(["mail-gmail"]);
  expect(readyConnectors(["gmail"], list, ["mail-gmail"])).toEqual([]);
  expect(readyConnectors(["slack"], list, [])).toEqual([]);
});

it("offers one connection per mentioned service, and none when the service is already connected", () => {
  expect(connectOffers(["gmail"], list)).toEqual([]);
  expect(connectOffers(["slack", "gmail"], list).map(c => c.id)).toEqual(["slack"]);
  const none = [info("mail-gmail", "gmail", false), info("gmail", "gmail", false)];
  expect(connectOffers(["gmail", "mail-gmail"], none).map(c => c.id)).toEqual(["gmail"]);
});

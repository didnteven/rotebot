import type { ConnectorInfo } from "./backend/types";

const serviceOf = (c: ConnectorInfo) => c.service ?? c.id;

/** Connected connectors to switch on: one per mentioned service not yet selected (Gmail by app password or by Google sign-in is one service). */
export function readyConnectors(mentionedIds: string[], connectors: ConnectorInfo[], selectedIds: string[]): string[] {
  const byId = new Map(connectors.map(c => [c.id, c]));
  const have = new Set(selectedIds.map(id => (byId.has(id) ? serviceOf(byId.get(id)!) : id)));
  const services = new Set(mentionedIds.map(id => (byId.has(id) ? serviceOf(byId.get(id)!) : id)));
  const out: string[] = [];
  for (const service of services) {
    if (have.has(service)) continue;
    const connected = connectors.find(c => serviceOf(c) === service && c.state.kind === "connected");
    if (connected) out.push(connected.id);
  }
  return out;
}

/** One connector to offer per mentioned service that has no connected connection at all. */
export function connectOffers(mentionedIds: string[], connectors: ConnectorInfo[]): ConnectorInfo[] {
  const seen = new Set<string>();
  const out: ConnectorInfo[] = [];
  for (const id of mentionedIds) {
    const c = connectors.find(x => x.id === id);
    if (!c) continue;
    const service = serviceOf(c);
    if (seen.has(service)) continue;
    seen.add(service);
    if (!connectors.some(x => serviceOf(x) === service && x.state.kind === "connected")) out.push(c);
  }
  return out;
}

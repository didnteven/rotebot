import { useEffect, useRef, useState } from "react";
import type { AccessPreview, ApiConfig, ApiProposal, Backend, ConnectorInfo, ModelAccessStatus, TaskDraft } from "./backend/types";
import { Field, useAction } from "./ui";

const names = (text: string) => text.split(",").map(n => n.trim()).filter(Boolean);

/** No credential field: the native backend asks for each secret after the address is reviewed. */
export function ApiSetup({ backend, proposal, connected, changed }: {
  backend: Backend; proposal?: ApiProposal; connected?: (info: ConnectorInfo) => Promise<void>; changed?: () => Promise<void>;
}) {
  const [config, setConfig] = useState<ApiConfig>({ name: proposal?.name ?? "", url: proposal?.url ?? "",
    auth: proposal?.auth === "none" ? "none" : proposal?.auth === "header" ? "header" : "bearer",
    headerName: proposal?.headerName || "X-API-Key", docsUrl: proposal?.docsUrl ?? null, extraHeaderNames: proposal?.extraHeaderNames ?? [] });
  const [extraText, setExtraText] = useState((proposal?.extraHeaderNames ?? []).join(", "));
  const [saved, setSaved] = useState<ConnectorInfo | null>(null);
  const [notice, setNotice] = useState("");
  const { act, busy, error } = useAction();
  const extras = config.auth === "none" ? [] : config.extraHeaderNames ?? [];
  const keys = config.auth === "none" ? 0 : 1 + extras.length;
  const invalid = !config.name.trim() || !config.url.startsWith("https://") || /[{}]/.test(config.url);
  return <section aria-label="Custom API setup" className="card">
    <h3>{proposal ? `Connect ${proposal.name}` : "Add a custom API"}</h3>
    <p>Macroscope will only read from this service. It can look at your data but never change it.</p>
    <ol>
      <li>{keys === 0 ? "No key is needed." : keys === 1 ? "Save it. A secure box opens where you paste your key." : `Save it. Secure boxes open one at a time where you paste your ${keys} keys.`}</li>
      <li>Test it. Macroscope makes one read request to check that it works.</li>
      <li>Pick it for your task.</li>
    </ol>
    <fieldset disabled={busy || !!saved}>
      <Field label="Name"><input value={config.name} maxLength={80} onChange={e => setConfig({ ...config, name: e.target.value })} /></Field>
      <details open={!proposal}><summary>Technical details{proposal ? " (already filled in)" : ""}</summary>
        <p className="muted">Only change these if the service's own instructions say so.</p>
        <Field label="API URL"><input type="url" value={config.url} onChange={e => setConfig({ ...config, url: e.target.value })} /></Field>
        <Field label="How it checks who you are"><select value={config.auth} onChange={e => setConfig({ ...config, auth: e.target.value as ApiConfig["auth"] })}>
          <option value="none">Nothing, it is open to everyone</option><option value="bearer">An access token</option><option value="header">A key sent in a header</option>
        </select></Field>
        {config.auth === "header" && <Field label="Header name for the key"><input value={config.headerName} onChange={e => setConfig({ ...config, headerName: e.target.value })} /></Field>}
        {config.auth !== "none" && <Field label="Other keys it also needs, by header name (comma separated, optional)"><input value={extraText} placeholder="X-App-Id"
          onChange={e => { setExtraText(e.target.value); setConfig({ ...config, extraHeaderNames: names(e.target.value) }); }} /></Field>}
        <Field label="Link to the service's instructions"><input type="url" value={config.docsUrl ?? ""} onChange={e => setConfig({ ...config, docsUrl: e.target.value || null })} /></Field>
      </details>
    </fieldset>
    <p>Your keys are kept in the app's secret store. They are never shown on screen or sent to the task model.</p>
    {!saved ? <button disabled={busy || invalid} onClick={() => void act(async () => {
      const info = await backend.saveApiConnection({ ...config, extraHeaderNames: extras }); setSaved(info);
      setNotice("Saved on this Mac. Now test it before using it."); await changed?.();
    })}>Save API connection</button> : <>
      <p>Testing makes one read request to <bdi>{saved.mcpUrl}</bdi> and checks that a complete answer comes back.</p>
      <button disabled={busy || saved.state.kind === "connected"} onClick={() => void act(async () => {
        try { const info = await backend.testConnection(saved.id); setSaved(info); await connected?.(info); setNotice("It works. Review exactly what the tool reads before building."); }
        finally { await changed?.(); }
      })}>Test API access</button>
    </>}
    {busy && <p role="status">Working…</p>}{notice && <p role="status">{notice}</p>}{error && <p role="alert">{error}</p>}
  </section>;
}

const costs: Record<string, string> = { public: "Free, no account needed", free_personal: "Free for your own accounts or data",
  free_tier: "Has a free plan", paid_or_approval: "May cost money or need the provider's approval" };
const costLabel = (cost?: string) => costs[cost ?? ""] ?? "Cost not confirmed in the documentation";

const states: Record<string, string> = { local: "Local work", existing_connection: "Use an existing connection", public_api: "Public API found",
  connect_service: "Set up an available connection",
  needs_connection: "API setup needed", needs_details: "API details needed", needs_adapter: "Additional adapter needed", needs_research: "Access still needs research" };

export function ApiAccessPanel({ backend, draft, status, accept, changed, onBusy, connect, connectors = [] }: {
  backend: Backend; draft: TaskDraft; status: ModelAccessStatus; accept: (draft: TaskDraft) => void;
  changed?: () => Promise<void>; onBusy: (busy: boolean) => void;
  connect?: (id: string) => void; connectors?: ConnectorInfo[];
}) {
  const [preview, setPreview] = useState<AccessPreview | null>(null);
  const [previewError, setPreviewError] = useState<string | null>(null);
  const [attempt, setAttempt] = useState(0);
  const { act, busy, error } = useAction();
  const current = useRef(draft.draftId);
  current.current = draft.draftId;
  const mounted = useRef(true);
  useEffect(() => { mounted.current = true; return () => { mounted.current = false; }; }, []);
  useEffect(() => { onBusy(busy); return () => onBusy(false); }, [busy, onBusy]);
  const connections = JSON.stringify(draft.requiredConnections ?? []);
  useEffect(() => {
    let active = true; setPreview(null); setPreviewError(null);
    void backend.accessPreview(draft.draftId).then(p => { if (active) setPreview(p); })
      .catch(e => { if (active) setPreviewError(e instanceof Error ? e.message : "Couldn't prepare access research"); });
    return () => { active = false; };
  }, [backend, draft.draftId, draft.goal, connections, status.effectiveRoute, status.egress, attempt]);
  const plan = draft.accessPlan;
  const api = plan?.api;
  const canSetup = api?.method === "GET" && ["none", "bearer", "header"].includes(api.auth);
  const paused = status.egress !== "cloud_allowed" || status.effectiveRoute === "offline_demo" || !status.effectiveRoute;
  async function selectConnection(id: string) {
    const ids = [...new Set([...(draft.requiredConnections ?? []).map(c => c.connectorId), id])];
    const next = await backend.selectTaskConnections(draft.draftId, ids);
    if (mounted.current && current.current === draft.draftId) accept(next);
    await changed?.();
  }
  return <section aria-label="Task access" className="card">
    <h3>How this task gets its data</h3>
    <p>Check suitable existing connections and known public APIs first. Research missing access and get setup instructions when needed.</p>
    {plan && <div role="note">
      <p><strong>{states[plan.state] ?? "Access check"}</strong></p><p>{plan.summary}</p>
      <p className="muted">{plan.searched ? "Web research was used." : "Existing access and known documentation were checked."} API access and task results still need testing.</p>
      {plan.connectorId && (plan.state !== "connect_service" || connectors.some(c => c.id === plan.connectorId && c.state.kind === "connected")) && <button disabled={busy || paused} onClick={() => void act(() => selectConnection(plan.connectorId!))}>Use this existing connection</button>}
      {plan.connectorId && plan.state === "connect_service" && !connectors.some(c => c.id === plan.connectorId && c.state.kind === "connected") && <button disabled={busy || !connect} onClick={() => connect?.(plan.connectorId!)}>Set up this available connection</button>}
      {api && <>
        <p className="badge">{costLabel(api.cost)}</p>
        <p><bdi>{api.method} {api.url}</bdi></p><p>{api.coverage}</p>
        {!!api.prerequisites.length && <><h4>What you need</h4><ul>{api.prerequisites.map((s, i) => <li key={i}>{s}</li>)}</ul></>}
        {!!api.setupInstructions.length && <><h4>Setup instructions</h4><ol>{api.setupInstructions.map((s, i) => <li key={i}>{s}</li>)}</ol></>}
      </>}
      {plan.sources.map(url => <button key={url} className="quiet" onClick={() => void act(async () => {
        if (window.confirm(`Open this documentation in your browser?\n\n${url}`)) await backend.openExternal(url);
      })}>Open documentation: {url}</button>)}
      {plan.state === "public_api" && <p>No credential setup is needed. The tool's source and result will be checked during the build.</p>}
      {api && canSetup && ["needs_connection", "needs_details"].includes(plan.state) && <ApiSetup key={`${draft.draftId}:${api.url}`} backend={backend} proposal={api} changed={changed} connected={info => selectConnection(info.id)} />}
      {api && !canSetup && <p>This API needs an adapter for its authentication or operation. A key-entry form cannot enable it yet.</p>}
    </div>}
    <details><summary>Review access research</summary>
      <p>Sending this check shares your task description and connection metadata with {preview?.recipient ?? "your selected model"}. File contents, recordings and credentials are excluded. Missing access may trigger service/action searches and up to two documentation reads. It will not read connected accounts or connect anything.</p>
      <p>Research uses up to two model calls and three hosted search tool calls, with a limit of 100 research model calls per month. Provider search fees may apply; their price is not included in the tool-build estimate.</p>
      {preview && <pre aria-label="Access research data" tabIndex={0}>{preview.payloadText}</pre>}
    </details>
    {paused && <p>Choose a model and allow cloud access to check API options.</p>}
    {previewError && <p role="alert">{previewError} <button onClick={() => setAttempt(n => n + 1)}>Retry access preview</button></p>}
    <button disabled={busy || paused || !preview} onClick={() => void act(async () => {
      const id = draft.draftId; const next = await backend.researchTaskAccess(id, preview!.payloadHash);
      if (mounted.current && current.current === id) accept(next);
    })}>{busy ? "Checking access…" : plan ? "Check access again" : "Send and check access"}</button>
    {!preview?.searchAvailable && !paused && <details><summary>Enable broader API searches</summary>
      <p>This model can assess known APIs and existing connections. Broader web research needs your own Brave Search API key. Search labels are sent to Brave; your key stays in the app's secret store.</p>
      <button onClick={() => void act(async () => { if (window.confirm("Open Brave's search API dashboard?")) await backend.openExternal("https://api-dashboard.search.brave.com/"); })}>Get search API instructions</button>
      <button disabled={busy} onClick={() => void act(async () => { await backend.saveApiSearchKey(); setAttempt(n => n + 1); })}>Enter search key securely</button>
    </details>}
    {preview?.searchAvailable && status.effectiveRoute !== "api_key" && <button className="quiet" disabled={busy} onClick={() => void act(async () => { await backend.removeApiSearchKey(); setAttempt(n => n + 1); })}>Remove research search key</button>}
    {error && <p role="alert">{error}</p>}
  </section>;
}

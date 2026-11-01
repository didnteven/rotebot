import { BotStatus } from "./Bot";
import { useEffect, useRef, useState } from "react";
import type { AccessPreview, ApiConfig, ApiProposal, Backend, ConnectorInfo, ModelAccessStatus, TaskDraft } from "./backend/types";
import { Field, useAction } from "./ui";

/** Shown in a key box while a saved key is kept; saving it unchanged keeps the saved key. */
const KEPT = "••••••••••";
const names = (text: string) => text.split(",").map(n => n.trim()).filter(Boolean);

/** No credential field: the native backend asks for each secret after the address is reviewed. */
export function ApiSetup({ backend, proposal, connected, changed, edit, done }: {
  backend: Backend; proposal?: ApiProposal; connected?: (info: ConnectorInfo) => Promise<void>; changed?: () => Promise<void>;
  /** Reopens a saved API: its settings are filled in, and saving replaces it (keys are typed again). */
  edit?: { id: string; config: ApiConfig }; done?: () => void;
}) {
  const blank = (): ApiConfig => edit ? { ...edit.config, extraHeaderNames: edit.config.extraHeaderNames ?? [] } : ({ name: proposal?.name ?? "", url: proposal?.url ?? "",
    auth: proposal?.auth === "none" ? "none" : proposal?.auth === "header" || proposal?.auth === "query" ? proposal.auth : "bearer",
    headerName: proposal?.headerName || "X-API-Key", docsUrl: proposal?.docsUrl ?? null, extraHeaderNames: proposal?.extraHeaderNames ?? [] });
  const [config, setConfig] = useState<ApiConfig>(blank);
  const [extraText, setExtraText] = useState((edit?.config.extraHeaderNames ?? proposal?.extraHeaderNames ?? []).join(", "));
  const [saved, setSaved] = useState<ConnectorInfo | null>(null);
  const kept = () => edit && edit.config.auth !== "none" ? KEPT : "";
  const keptExtras = () => Object.fromEntries((edit?.config.extraHeaderNames ?? []).map(n => [n, KEPT]));
  const [mainValue, setMainValue] = useState(kept);
  const [extraValues, setExtraValues] = useState<Record<string, string>>(keptExtras);
  const [notice, setNotice] = useState("");
  const { act, busy, error } = useAction();
  const extras = config.auth === "none" ? [] : config.extraHeaderNames ?? [];
  const keys = config.auth === "none" ? 0 : 1 + extras.length;
  const mainKey = config.auth === "bearer" ? "your access token (sent as Authorization: Bearer …)"
    : config.auth === "query" ? `your key (added to the address as ?${config.headerName}=…)` : `your key (sent in the ${config.headerName} header)`;
  const invalid = !config.name.trim() || !config.url.startsWith("https://") || /[{}]/.test(config.url)
    || (config.auth !== "none" && !edit && (!mainValue.trim() || extras.some(n => !extraValues[n]?.trim())));
  return <section aria-label="Custom API setup" className="card">
    <h3>{edit ? `Edit ${edit.config.name}` : proposal ? `Connect ${proposal.name}` : "Add a custom API"}</h3>
    {edit && <p className="muted">Your saved keys show as dots and are kept unless you type new ones. Saving replaces the old connection.</p>}
    <p>Macroscope will only read from this service. It can look at your data but never change it.</p>
    <ol>
      <li>{keys === 0 ? "No key is needed." : "Paste your key below and save it."}</li>
      <li>Test it. Macroscope makes one read request to check that it works.</li>
      <li>Pick it for your task.</li>
    </ol>
    <fieldset disabled={busy || !!saved}>
      <Field label="Name"><input value={config.name} maxLength={80} onChange={e => setConfig({ ...config, name: e.target.value })} /></Field>
      <details open={!proposal}><summary>Technical details{proposal ? " (already filled in)" : ""}</summary>
        <p className="muted">Only change these if the service's own instructions say so.</p>
        <Field label="API URL (an address on the service; other paths on it can use the same keys)"><input type="url" value={config.url} onChange={e => setConfig({ ...config, url: e.target.value })} /></Field>
        <Field label="How it checks who you are"><select value={config.auth} onChange={e => { const auth = e.target.value as ApiConfig["auth"]; setConfig({ ...config, auth, headerName: auth === "query" && config.headerName === "X-API-Key" ? "access_token" : auth === "header" && config.headerName === "access_token" ? "X-API-Key" : config.headerName }); }}>
          <option value="none">Nothing, it is open to everyone</option><option value="bearer">An access token</option><option value="header">A key in a header</option><option value="query">A key in the address</option>
        </select></Field>
        {(config.auth === "header" || config.auth === "query") && <Field label={config.auth === "query" ? "Parameter name for the key" : "Header name for the key"}><input value={config.headerName} onChange={e => setConfig({ ...config, headerName: e.target.value })} /></Field>}
        {config.auth !== "none" && <Field label={config.auth === "query" ? "Second key, by parameter name (optional)" : "Second key, by header name (optional)"}><input value={extraText} placeholder={config.auth === "query" ? "app_id" : "X-App-Id"}
          onChange={e => { setExtraText(e.target.value); setConfig({ ...config, extraHeaderNames: names(e.target.value) }); }} /></Field>}
        <Field label="Link to the service's instructions"><input type="url" value={config.docsUrl ?? ""} onChange={e => setConfig({ ...config, docsUrl: e.target.value || null })} /></Field>
      </details>
    </fieldset>
    <p>Your keys are kept in the app's secret store. They are never shown again or sent to the task model.</p>
    {!saved && keys > 0 && <fieldset disabled={busy} aria-label="Your keys">
      <legend>Your keys</legend>
      <Field label={`Main key: ${mainKey}`}><input type="password" autoComplete="off" value={mainValue} onChange={e => setMainValue(e.target.value)} /></Field>
      {extras.map(n => <Field key={n} label={`Second key: the value for ${n}`}><input type="password" autoComplete="off" value={extraValues[n] ?? ""} onChange={e => setExtraValues({ ...extraValues, [n]: e.target.value })} /></Field>)}
    </fieldset>}
    {!saved ? <button disabled={busy || invalid} onClick={() => void act(async () => {
      const typed = (v: string) => v === KEPT ? "" : v.trim();
      const keys = config.auth === "none" ? undefined : { main: typed(mainValue), extras: extras.map(n => typed(extraValues[n] ?? "")) };
      const info = edit ? await backend.saveApiConnection({ ...config, extraHeaderNames: extras }, keys, edit.id) : await backend.saveApiConnection({ ...config, extraHeaderNames: extras }, keys);
      setMainValue(""); setExtraValues({}); setSaved(info);
      setNotice("Saved on this Mac. Now test it before using it."); await changed?.();
    })}>Save API connection</button> : <>
      <p>Testing makes one read request to <bdi>{saved.mcpUrl}</bdi> and checks that a complete answer comes back.</p>
      <button disabled={busy} onClick={() => void act(async () => {
        try { const info = await backend.testConnection(saved.id); setSaved(info); await connected?.(info); setNotice("It works. Review exactly what the tool reads before building."); }
        finally { await changed?.(); }
      })}>Test API access</button>
      <div className="actions">
        {saved.state.kind !== "connected" && <button disabled={busy} onClick={() => void act(async () => {
          // A saved connection never changes, so editing replaces it: remove it, keep the details, ask for the keys again.
          await backend.disconnect(saved.id); setSaved(null); setNotice(""); await changed?.();
        })}>Change details</button>}
        {done && <button disabled={busy} onClick={done}>Done</button>}
        <button disabled={busy} onClick={() => { setSaved(null); setConfig(blank()); setExtraText((proposal?.extraHeaderNames ?? []).join(", ")); setMainValue(kept()); setExtraValues(keptExtras()); setNotice(""); }}>Add another API</button>
      </div>
    </>}
    {busy && <BotStatus label="Updating API access…" />}{notice && <p role="status">{notice}</p>}{error && <p role="alert">{error}</p>}
  </section>;
}

const costs: Record<string, string> = { public: "Free, no account needed", free_personal: "Free for your own accounts or data",
  free_tier: "Has a free plan", paid_or_approval: "May cost money or need the provider's approval" };
const costLabel = (cost?: string) => costs[cost ?? ""] ?? "Cost not confirmed in the documentation";

const states: Record<string, string> = { local: "Local work", existing_connection: "Use an existing connection", public_api: "Public API found",
  connect_service: "Set up an available connection",
  same_service_works: "Your existing keys work here",
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
  const autoRan = useRef<string | null>(null);
  const current = useRef(draft.draftId);
  current.current = draft.draftId;
  const mounted = useRef(true);
  useEffect(() => { mounted.current = true; return () => { mounted.current = false; }; }, []);
  // Until the automatic check has run (or failed), building waits for it.
  const pendingCheck = !draft.accessPlan && !(status.egress !== "cloud_allowed" || status.effectiveRoute === "offline_demo" || !status.effectiveRoute)
    && !previewError && !error && (!preview || autoRan.current !== preview.payloadHash);
  useEffect(() => { onBusy(busy || pendingCheck); return () => onBusy(false); }, [busy, pendingCheck, onBusy]);
  const connections = JSON.stringify(draft.requiredConnections ?? []);
  useEffect(() => {
    let active = true; setPreview(null); setPreviewError(null);
    void backend.accessPreview(draft.draftId).then(p => { if (active) setPreview(p); })
      .catch(e => { if (active) setPreviewError(e instanceof Error ? e.message : "Couldn't prepare access research"); });
    return () => { active = false; };
  }, [backend, draft.draftId, draft.goal, connections, status.effectiveRoute, status.egress, attempt]);
  const plan = draft.accessPlan;
  const api = plan?.api;
  const canSetup = api?.method === "GET" && ["none", "bearer", "header", "query"].includes(api.auth);
  const paused = status.egress !== "cloud_allowed" || status.effectiveRoute === "offline_demo" || !status.effectiveRoute;
  async function selectConnection(id: string) {
    const ids = [...new Set([...(draft.requiredConnections ?? []).map(c => c.connectorId), id])];
    const next = await backend.selectTaskConnections(draft.draftId, ids);
    if (mounted.current && current.current === draft.draftId) accept(next);
    await changed?.();
  }
  // The person accepted their AI service once, so the check runs on its own instead of asking first.
  useEffect(() => {
    if (plan || paused || !preview || busy || autoRan.current === preview.payloadHash) return;
    autoRan.current = preview.payloadHash;
    const id = draft.draftId;
    void act(async () => { const next = await backend.researchTaskAccess(id, preview.payloadHash); if (mounted.current && current.current === id) accept(next); });
  }, [plan, paused, preview, busy]);
  const using = (id?: string | null) => !!id && (draft.requiredConnections ?? []).some(c => c.connectorId === id);
  const chosen = plan?.state === "same_service_works" ? using(plan.reuseKeysFrom) : plan?.connectorId ? using(plan.connectorId) : false;
  return <section aria-label="Task access" className="card api-access">
    <h3>Where this task gets its data</h3>
    {plan && <div role="note" className="api-plan">
      <p><strong>{states[plan.state] ?? "Access check"}</strong>{chosen && <span className="badge">In use</span>}</p>
      <p>{plan.summary}</p>
      {api && <p className="api-address"><bdi>{api.method} {api.url}</bdi></p>}
      {plan.connectorId && !chosen && (plan.state !== "connect_service" || connectors.some(c => c.id === plan.connectorId && c.state.kind === "connected")) && <button className="primary" disabled={busy || paused} onClick={() => void act(() => selectConnection(plan.connectorId!))}>Use this existing connection</button>}
      {plan.connectorId && plan.state === "connect_service" && !connectors.some(c => c.id === plan.connectorId && c.state.kind === "connected") && <button className="primary" disabled={busy || !connect} onClick={() => connect?.(plan.connectorId!)}>Set up this available connection</button>}
      {api && plan.state === "same_service_works" && plan.reuseKeysFrom && !chosen && <button className="primary" disabled={busy} onClick={() => void act(async () => {
        const info = await backend.addApiAddress(plan.reuseKeysFrom!, api.url); await selectConnection(info.id);
      })}>Use my existing connection for this</button>}
      {chosen && <p role="status">Ready. You can send and build below.</p>}
      {plan.state === "public_api" && <p>No credential setup is needed. The tool's source and result will be checked during the build.</p>}
      {api && <><p><span className="badge">{costLabel(api.cost)}</span></p><p className="muted">{api.coverage}</p></>}
      {api && !chosen && (!!api.prerequisites.length || !!api.setupInstructions.length) && <details open>
        <summary>Setup steps</summary>
        {!!api.prerequisites.length && <><h4>What you need</h4><ul>{api.prerequisites.map((s, i) => <li key={i}>{s}</li>)}</ul></>}
        {!!api.setupInstructions.length && <><h4>Setup instructions</h4><ol>{api.setupInstructions.map((s, i) => <li key={i}>{s}</li>)}</ol></>}
      </details>}
      {api && canSetup && ["needs_connection", "needs_details"].includes(plan.state) && <ApiSetup key={`${draft.draftId}:${api.url}`} backend={backend} proposal={api} changed={changed} connected={info => selectConnection(info.id)} />}
      {api && !canSetup && <p>This API needs an adapter for its authentication or operation. A key-entry form cannot enable it yet.</p>}
      {!!plan.sources.length && <details open><summary>Documentation</summary>
        {plan.sources.map(url => <button key={url} className="quiet" onClick={() => void act(async () => {
          if (window.confirm(`Open this documentation in your browser?\n\n${url}`)) await backend.openExternal(url);
        })}>Open documentation: {url}</button>)}
      </details>}
    </div>}
    {paused && !plan && <p className="muted">Set up an AI service in Settings to find where this data comes from.</p>}
    {previewError && <p role="alert">{previewError} <button onClick={() => setAttempt(n => n + 1)}>Retry access preview</button></p>}
    <details className="api-more"><summary>What this check sends</summary>
      <p className="muted">This check sends your task description and connection names to {preview?.recipient ?? "your AI service"}, never file contents or keys.</p>
      <details><summary>Review access research</summary>{preview && <pre aria-label="Access research data" tabIndex={0}>{preview.payloadText}</pre>}</details>
      {!preview?.searchAvailable && !paused && <details><summary>Enable broader API searches</summary>
        <p>This model can assess known APIs and existing connections. Broader web research needs your own Brave Search API key. Search labels are sent to Brave; your key stays in the app's secret store.</p>
        <button onClick={() => void act(async () => { if (window.confirm("Open Brave's search API dashboard?")) await backend.openExternal("https://api-dashboard.search.brave.com/"); })}>Get search API instructions</button>
        <button disabled={busy} onClick={() => void act(async () => { await backend.saveApiSearchKey(); setAttempt(n => n + 1); })}>Enter search key securely</button>
      </details>}
      {preview?.searchAvailable && status.effectiveRoute !== "api_key" && <button className="quiet" disabled={busy} onClick={() => void act(async () => { await backend.removeApiSearchKey(); setAttempt(n => n + 1); })}>Remove research search key</button>}
    </details>
    {plan && <button className="quiet" disabled={busy || paused || !preview} onClick={() => void act(async () => {
      const id = draft.draftId; const next = await backend.researchTaskAccess(id, preview!.payloadHash);
      if (mounted.current && current.current === id) accept(next);
    })}>Check again</button>}
    {busy && <BotStatus label="Finding where this data comes from…" />}
    {error && <p role="alert">{error}</p>}
  </section>;
}

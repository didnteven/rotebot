import { BotStatus } from "./Bot";
import { useEffect, useRef, useState } from "react";
import type {
  Backend,
  ConnectStart,
  ConnectorInfo,
  ConnectorTool,
} from "./backend/types";
import { Field, isHttpsUrl, Notice, useAction } from "./ui";
import { googleConnectorIds, parseGoogleClientFile } from "./clientFile";
import { ApiSetup } from "./ApiAccess";
import { Icon, serviceIcon, type IconName } from "./Icon";

function ServiceMark({ connector }: { connector: ConnectorInfo }) {
  const icon: IconName = serviceIcon(connector);
  return <span className={`service-mark service-${icon}`}><Icon name={icon} /></span>;
}

const stateName = (c: ConnectorInfo) =>
  c.state.kind === "connected"
    ? "✓ Connected"
    : c.state.kind === "not_connected"
      ? "○ Not connected"
      : c.state.kind === "connecting"
        ? "◷ Connecting"
        : c.state.kind === "needs_reauth"
          ? "! Sign in again"
          : "! Connection error";
export function Connections({
  backend,
  connectors,
  selected,
  select,
  changed,
  autoStart = false,
  loading = false,
}: {
  backend: Backend;
  connectors: ConnectorInfo[];
  selected: string | null;
  select: (id: string) => void;
  changed: () => Promise<void>;
  /** Start signing in to the selected service as soon as it opens (set when it was chosen elsewhere). */
  autoStart?: boolean;
  loading?: boolean;
}) {
  // Every click on a card, even the one already open, brings its details into view.
  const [tick, setTick] = useState(0);
  const [startTick, setStartTick] = useState(autoStart ? 1 : 0);
  const [addingApi, setAddingApi] = useState(false);
  const apis = connectors.filter((c) => c.registration === "api");
  return (
    <section className="connections-page">
      <div className="section-intro"><div><h2>Your services</h2><p>Connect once. Choose what each tool can use.</p></div>{!loading && <span className="status-badge status-success">{connectors.filter(c => c.state.kind === "connected").length} connected</span>}</div>
      {loading && <BotStatus label="Loading connections…" />}
      {!loading && connectors.length === 0 && <p>No services available yet. You can add a custom API below.</p>}
      {(() => {
        const card = (c: ConnectorInfo) => (
          <article className={`connector-row${selected === c.id ? " card-open" : ""}`} key={c.id}>
            <div className="connector-row-head">
              <ServiceMark connector={c} />
              <div className="connector-identity"><h2>{c.name}</h2><p>{c.category}</p></div>
              <span className={`status-badge connection-state-${c.state.kind}`}>{stateName(c)}</span>
              <button
                className={c.state.kind === "connected" ? "quiet" : undefined}
                aria-label={c.state.kind === "connected" ? `Manage ${c.name}` : `Connect ${c.name}`}
                aria-expanded={selected === c.id}
                onClick={() => {
                  select(c.id);
                  setTick((t) => t + 1);
                  // Choosing a service that isn't connected starts its sign-in when it can.
                  if (c.state.kind !== "connected") setStartTick((t) => t + 1);
                }}
              >
                {c.state.kind === "connected" ? "Manage" : "Connect"}
              </button>
            </div>
            {selected === c.id && (
              <>
                <ConnectorTools backend={backend} connectorId={c.id} />
                <ConnectionDetail
                  key={c.id}
                  backend={backend}
                  connector={c}
                  changed={changed}
                  focusTick={tick}
                  startTick={startTick}
                />
              </>
            )}
          </article>
        );
        // Rows stay in one place whatever their state, so a sign-in in progress is never restarted by a move.
        const services = connectors.filter((c) => c.registration !== "api");
        const custom = services.filter((c) => c.category.toLowerCase() === "custom");
        const others = services.filter((c) => c.category.toLowerCase() !== "custom");
        return <>
          {others.length > 0 && <div className="connector-list">{others.map(card)}</div>}
          {custom.length > 0 && <details className="advanced-connections" open={custom.some(c => c.id === selected || c.state.kind !== "not_connected") || undefined}><summary>Advanced</summary><div className="connector-list">{custom.map(card)}</div></details>}
        </>;
      })()}
      <section aria-labelledby="custom-apis">
        <h2 id="custom-apis" className="list-heading">Custom APIs</h2>
        {apis.length === 0 && <p className="muted">Not sure what to connect? Describe your task and Rotebot finds the API for you.</p>}
        <div className="connector-list">
          {apis.map((c) => <ApiCard key={c.id} backend={backend} connector={c} changed={changed} />)}
        </div>
        {addingApi
          ? <ApiSetup backend={backend} changed={changed} />
          : <button onClick={() => setAddingApi(true)}>Add a custom API</button>}
      </section>
    </section>
  );
}
/** One saved API: name, status and Test up front; address, editing, tools and debugging behind Manage. */
function ApiCard({ backend, connector, changed }: { backend: Backend; connector: ConnectorInfo; changed: () => Promise<void> }) {
  const failing = connector.state.kind === "error" || connector.state.kind === "needs_reauth";
  const [open, setOpen] = useState(failing);
  const { act, busy, error } = useAction();
  const [tested, setTested] = useState("");
  const [testing, setTesting] = useState(false);
  const [editing, setEditing] = useState<import("./backend/types").ApiConfig | null>(null);
  return <article className={`connector-row${open ? " card-open" : ""}`}>
    <div className="connector-row-head">
      <ServiceMark connector={connector} />
      <div className="connector-identity"><h3>{connector.name}</h3><p>Custom API</p></div>
      <span className={`status-badge connection-state-${connector.state.kind}`}>{stateName(connector)}</span>
      <button className="quiet" disabled={busy} aria-label="Test API connection" onClick={() => void act(async () => { setTested(""); setTesting(true); try { await backend.testConnection(connector.id); setTested("Tested just now: it works."); } finally { setTesting(false); await changed(); } })}>Test</button>
      <button className="quiet" aria-expanded={open} aria-label={`Manage ${connector.name}`} onClick={() => setOpen(!open)}>{open ? "Close" : "Manage"}</button>
    </div>
    {testing && <BotStatus label="Testing the API with your saved keys…" />}
    {tested && !error && <p className="notice notice-success" role="status">{tested}</p>}
    {connector.state.kind === "error" && <p role="alert">{connector.state.message}</p>}
    {error && <p role="alert">{error}</p>}
    {open && (editing
      ? <ApiSetup backend={backend} changed={changed} edit={{ id: connector.id, config: editing }} done={() => setEditing(null)} />
      : <div className="api-manage">
          <p className="muted"><bdi>{connector.mcpUrl}</bdi></p>
          <div className="actions">
            <button disabled={busy} onClick={() => void act(async () => setEditing(await backend.getApiConfig(connector.id)))}>Edit API connection</button>
            <button className="quiet danger" disabled={busy} onClick={() => void act(async () => { await backend.disconnect(connector.id); await changed(); })}>Remove API connection</button>
          </div>
          <ConnectorTools backend={backend} connectorId={connector.id} />
          <ApiExamples backend={backend} id={connector.id} current={connector.mcpUrl} changed={changed} failing={failing} />
          {busy && <BotStatus label="Updating API connection…" />}
        </div>)}
  </article>;
}
/** Debug section of a saved API: explains why it is not working, repairs what it can, and finds addresses that do work. */
function ApiExamples({ backend, id, current, failing, changed }: { backend: Backend; id: string; current: string; failing?: boolean; changed: () => Promise<void> }) {
  const [examples, setExamples] = useState<import("./backend/types").ApiExample[]>([]);
  const [report, setReport] = useState<import("./backend/types").ApiDebug | null>(null);
  const { act, busy, error } = useAction();
  const [working, setWorking] = useState("");
  const allChecked = examples.length > 0 && examples.every(e => e.ok !== null);
  const anyWorks = examples.some(e => e.ok === true);
  const run = (label: string, task: () => Promise<void>) => void act(async () => { setWorking(label); try { await task(); } finally { setWorking(""); } });
  useEffect(() => { void backend.listApiExamples(id).then(setExamples).catch(() => undefined); }, [backend, id]);
  const findWorking = async (inline = false) => {
    const task = async () => {
      setWorking("Looking for addresses that work: your model reads the documentation, then each candidate is tested with your saved keys. This can take a minute or two…");
      let list = await backend.suggestApiExamples(id); setExamples(list);
      for (const e of list) { if (e.ok === null) { list = await backend.runApiExample(id, e.url); setExamples(list); } }
    };
    if (inline) await task(); else run("Looking for addresses that work: your model reads the documentation, then each candidate is tested with your saved keys. This can take a minute or two…", task);
  };
  const better = examples.find(e => e.ok === true && e.url !== current);
  // A connection is its service and keys: a path that works is just remembered on it, never a second connection.
  async function useAddress(url: string) {
    await backend.addApiAddress(id, url);
    await changed();
  }
  return <details open={!!failing || !!report}><summary>Debug this connection</summary>
    <p className="muted">Finds what’s wrong and fixes what it can. Your keys and data are never sent to the AI.</p>
    <button disabled={busy} className="primary" onClick={() => run("Checking the address without keys, then with your saved keys…", async () => {
      const result = await backend.debugApiConnection(id); setReport(result);
      if (result.fixed) { try { await backend.testConnection(id); } finally { await changed(); } }
      else if (result.tryExamples) await findWorking(true);
    })}>Check and fix this connection</button>
    {busy && working && <BotStatus label={working} />}
    {better && (failing || report) && <div role="status" aria-label="Working address found">
      <h4>Found an address that works</h4>
      <p><bdi>{better.url}</bdi></p>
      <p className="muted">Your keys work here, so this connection can use this address instead.</p>
      <button className="primary" disabled={busy} onClick={() => run("Saving that path on this connection…", () => useAddress(better.url))}>Use this path on this connection</button>
    </div>}
    {report && <div aria-label="Debug report">
      <ol>{report.steps.map((step, i) => <li key={i}><strong>{step.ok ? "✓" : "✗"} {step.label}.</strong> {step.detail}</li>)}</ol>
      <p role="status"><strong>{report.verdict}</strong></p>
    </div>}
    {(report?.tryExamples || examples.length > 0 || failing) && <button className="quiet" disabled={busy} onClick={() => void findWorking()}>{examples.length ? "Find more working addresses" : "Find working addresses"}</button>}
    {allChecked && !anyWorks && <div role="alert" aria-label="Nothing worked">
      <h4>Every example was checked and none worked</h4>
      <p>First re-check your keys: choose <strong>Edit API connection</strong> above and paste them again, making sure each goes in the right box. Then check the address against the service's documentation. This is what each call said:</p>
      <ul>{examples.map(e => <li key={e.url}><bdi>{e.url}</bdi>: {e.result}</li>)}</ul>
    </div>}
    {examples.length > 0 && <details className="api-examples"><summary>Addresses checked · {examples.filter(e => e.ok).length} of {examples.length} work</summary>
    {examples.map(e => <article key={e.url} aria-label={e.label} className="api-example">
      <div className="api-example-head">
        <strong>{e.ok === true ? "✓" : e.ok === false ? "✗" : "○"} {e.label}</strong>
        <button className="quiet" disabled={busy} onClick={() => run(`Running “${e.label}” with your saved keys…`, async () => setExamples(await backend.runApiExample(id, e.url)))}>{e.ok === null ? "Run this example" : "Run again"}</button>
        {e.ok === true && e.url !== current && <button className="quiet" disabled={busy} onClick={() => run("Saving that path on this connection…", () => useAddress(e.url))}>Use this path</button>}
      </div>
      <p className="muted"><bdi>{e.url}</bdi></p>
      {e.ok === false && <p role="status" className="muted">Failed: <bdi>{e.result}</bdi></p>}
      {e.ok === true && <details><summary>What it returns</summary><p role="status" className="muted">Works. It returns: <bdi>{e.result}</bdi></p></details>}
    </article>)}
    </details>}
    {error && <p role="alert">{error}</p>}
  </details>;
}
function ConnectorTools({
  backend,
  connectorId,
}: {
  backend: Backend;
  connectorId: string;
}) {
  const [tools, setTools] = useState<ConnectorTool[] | null>(null);
  const requested = useRef(false);
  const { act, busy, error } = useAction();
  const load = () =>
    act(async () => {
      try {
        setTools(await backend.previewConnectorTools(connectorId));
      } catch (error) {
        requested.current = false;
        throw error;
      }
    });
  return (
    <details
      onToggle={(event) => {
        if (event.currentTarget.open && !requested.current) {
          requested.current = true;
          void load();
        }
      }}
    >
      <summary>What it can do</summary>
      {busy && <BotStatus label="Loading tools…" />}
      <Notice error={error} />
      {error && (
        <button disabled={busy} onClick={() => void load()}>
          Retry tools
        </button>
      )}
      {tools?.length === 0 && <p>Sign in to see its tools</p>}
      {tools && tools.length > 0 && (
        <ul>
          {tools.map((tool, index) => (
            <li key={index}>
              <strong>{tool.name}</strong>
              <p>{tool.description}</p>
            </li>
          ))}
        </ul>
      )}
    </details>
  );
}
function ConnectionDetail({
  backend,
  connector,
  changed,
  focusTick,
  startTick,
}: {
  backend: Backend;
  connector: ConnectorInfo;
  changed: () => Promise<void>;
  focusTick: number;
  startTick: number;
}) {
  const [url, setUrl] = useState(connector.mcpUrl),
    [confirmDocs, setConfirmDocs] = useState(false),
    [pending, setPending] = useState<ConnectStart | null>(null),
    [notice, setNotice] = useState<string | null>(null);
  const clientId = useRef<HTMLInputElement>(null),
    clientSecret = useRef<HTMLInputElement>(null),
    attempt = useRef(0);
  const { act, busy, error, setError } = useAction();
  const heading = useRef<HTMLHeadingElement>(null);
  const google = googleConnectorIds.includes(connector.id);
  const [shareGoogle, setShareGoogle] = useState(true);
  // One Google client works for Gmail, Calendar and Drive; each API still has to be switched on in Google Cloud.
  async function saveClient(id: string, secret: string | null) {
    await backend.setClientCredentials(connector.id, id, secret);
    if (google && shareGoogle) {
      for (const other of googleConnectorIds.filter((g) => g !== connector.id)) {
        try { await backend.setClientCredentials(other, id, secret); } catch { /* that service may be unavailable */ }
      }
    }
    setNotice(google && shareGoogle ? "Client credentials saved for Gmail, Calendar and Drive" : "Client credentials saved");
    await changed();
  }
  // Choosing a card opens its details inside the card: keep them in view.
  useEffect(() => {
    heading.current?.scrollIntoView?.({ block: "start", behavior: "smooth" });
    heading.current?.focus({ preventScroll: true });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [focusTick]);
  // Opens the service's own sign-in page in the browser as soon as nothing else is needed first.
  useEffect(() => {
    const ready =
      startTick > 0 &&
      !connector.mail &&
      connector.state.kind !== "connected" &&
      !!connector.mcpUrl &&
      (connector.registration !== "preregistered" || connector.hasClientCredentials);
    if (ready && !pending) void act(connect);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [startTick]);
  async function connect() {
    setNotice(null);
    const start = await backend.beginConnect(connector.id);
    const current = ++attempt.current;
    setPending(start);
    await changed();
    try {
      const result = await backend.completeConnect(start.pendingId);
      if (current === attempt.current) {
        setPending(null);
        setNotice(result.state.kind === "connected"
          ? `Connected · ${result.state.toolCount} tools available` : "Sign-in finished. Check connection status.");
        await changed();
      }
    } catch (e) {
      if (current === attempt.current) {
        setPending(null);
        await changed();
        throw e;
      }
    }
  }
  if (connector.mail) {
    return <MailDetail backend={backend} connector={connector} changed={changed} focusTick={focusTick} />;
  }
  return (
    <section
      className="connection-detail"
      aria-label={`${connector.name} connection details`}
    >
      <h3 ref={heading} tabIndex={-1}>{connector.name} connection</h3>
      {connector.registration === "preregistered" && !connector.hasClientCredentials && connector.state.kind !== "connected" && (
        <p className="notice notice-info" role="status">
          {connector.name} needs your own OAuth client before it can connect. Follow the steps below, save the client ID, then press Connect.
        </p>
      )}
      <Notice error={error} />
      {notice && (
        <p className="notice notice-success" role="status">
          {notice}
        </p>
      )}
      <form
        onSubmit={(e) => {
          e.preventDefault();
          void act(async () => {
            await backend.setConnectorUrl(connector.id, url);
            setNotice("MCP URL saved");
            await changed();
          });
        }}
      >
        <Field label="MCP URL">
          <input
            type="url"
            required
            pattern="https://.*"
            value={url}
            onChange={(e) => setUrl(e.target.value)}
          />
        </Field>
        <button disabled={busy || !!pending}>Save MCP URL</button>
      </form>
      <p>Current MCP URL: <code>{connector.mcpUrl}</code></p>
      {connector.docsUrl && (
        <p>
          {isHttpsUrl(connector.docsUrl) ? (
            <button type="button" onClick={() => setConfirmDocs(true)}>
              Documentation
            </button>
          ) : <>Documentation: <code>{connector.docsUrl}</code></>}
        </p>
      )}
      {confirmDocs && connector.docsUrl && isHttpsUrl(connector.docsUrl) && (
        <div className="notice notice-info" role="region" aria-label="Open documentation">
          <p>Open this address in your browser?</p>
          <p><code>{connector.docsUrl}</code></p>
          <div className="actions">
            <button disabled={busy} onClick={() => void act(async () => {
              await backend.openExternal(connector.docsUrl!);
              setConfirmDocs(false);
            })}>Open documentation in browser</button>
            <button onClick={() => setConfirmDocs(false)}>Keep here</button>
          </div>
        </div>
      )}
      {connector.registration === "preregistered" && (
        <form
          onSubmit={(e) => {
            e.preventDefault();
            const id = clientId.current?.value ?? "",
              secret = clientSecret.current?.value ?? "";
            if (clientId.current) clientId.current.value = "";
            if (clientSecret.current) clientSecret.current.value = "";
            void act(async () => {
              await saveClient(id, secret || null);
            });
          }}
        >
          {google ? (
            <>
              <h4>Set up Google sign-in (once)</h4>
              {connector.id === "gmail" ? (
                <ol>
                  <li>In <strong>console.cloud.google.com</strong>, create a project and turn on the <strong>Gmail API</strong> (APIs &amp; Services → Library).</li>
                  <li>Google Auth Platform → Get started: add an app name and your email. For Audience choose <strong>External</strong> (Internal only works with a Google Workspace organization), add a contact email, then agree and Create.</li>
                  <li>Audience → <strong>Test users</strong> → Add users: enter the exact Google address you will sign in with. Anyone not listed gets “Access blocked: has not completed the Google verification process”.</li>
                  <li>Data Access → Add or remove scopes: add <strong>gmail.modify</strong> (lets the tool read, draft, send and organise mail; each change is shown to you for approval first). If you connected before, Disconnect and Connect again to grant it.</li>
                  <li>Clients → Create client → <strong>Desktop app</strong>.</li>
                  <li>Download the JSON file and import it below, or copy the client ID and secret into the fields.</li>
                </ol>
              ) : (
              <ol>
                <li>In <strong>console.cloud.google.com</strong>, create a project.</li>
                <li>APIs &amp; Services → Library: turn on the {connector.name.replace("Google ", "")} API and its MCP API.</li>
                <li>Google Auth Platform → Get started: add an app name and your email. For Audience choose <strong>External</strong> (Internal only works with a Google Workspace organization), add a contact email, then agree and Create.</li>
                <li>Audience → <strong>Test users</strong> → Add users: enter the exact Google address you will sign in with. Anyone not listed gets “Access blocked: has not completed the Google verification process”.</li>
                <li>Data Access → Add or remove scopes: add the scopes this service needs.</li>
                <li>Clients → Create client → <strong>Web application</strong>, tick <strong>Use this client for an AI-powered agent</strong>, and add the Redirect URI shown below under Authorized redirect URIs.</li>
                <li>Google lists this connection as a Developer Preview for <strong>Google Workspace</strong> accounts: join at developers.google.com/workspace/preview with your Workspace account and Cloud project number (approval takes a couple of days).</li>
              </ol>
              )}
              <p className="muted">At sign-in Google may say it hasn’t verified the app: that is expected while it is in Testing mode, so choose Continue. This stays on your Mac. Until you publish the app in Google Cloud, Google asks you to sign in again about every 7 days.</p>
              <div>
                <label className="file-import">Import client file
                  <input type="file" accept=".json,application/json" disabled={busy} onChange={(e) => {
                    const file = e.target.files?.[0];
                    e.target.value = "";
                    if (!file) return;
                    void act(async () => {
                      const parsed = parseGoogleClientFile(await file.text());
                      if (!parsed) throw new Error("That isn't a Google OAuth client file. Download the JSON from Credentials in Google Cloud.");
                      await saveClient(parsed.clientId, parsed.clientSecret);
                    });
                  }} />
                </label>
              </div>
              <label className="recording-choice"><input type="checkbox" checked={shareGoogle} onChange={(e) => setShareGoogle(e.target.checked)} />Use the same client for Gmail, Calendar and Drive</label>
            </>
          ) : (
            connector.clientSetup && <p>{connector.clientSetup}</p>
          )}
          {connector.redirectUri && (
            <div>
              <Field label="Redirect URI">
                <input readOnly value={connector.redirectUri} />
              </Field>
              <button
                type="button"
                disabled={busy}
                onClick={() =>
                  void act(async () => {
                    await navigator.clipboard.writeText(connector.redirectUri!);
                    setNotice("Redirect URI copied");
                  })
                }
              >
                Copy
              </button>
            </div>
          )}
          <Field label="Client ID">
            <input ref={clientId} type="password" autoComplete="off" required />
          </Field>
          <Field label="Client secret">
            <input ref={clientSecret} type="password" autoComplete="off" />
          </Field>
          <button disabled={busy || !!pending}>Save client credentials</button>
          <p>
            {connector.hasClientCredentials
              ? "✓ Client credentials saved"
              : "Add your OAuth client ID"}
          </p>
        </form>
      )}
      {busy && !pending && <BotStatus label="Updating connection…" />}
      {pending ? (
        <>
          <BotStatus state="waiting" label="Finish signing in in your browser…" detail="Your bot is waiting for you to complete sign-in." />
          <Field label="Authorization URL">
            <input
              readOnly
              value={pending.authorizationUrl}
              onFocus={(e) => e.target.select()}
            />
          </Field>
          <button
            onClick={() => {
              ++attempt.current;
              setPending(null);
              setError(null);
              void backend
                .disconnect(connector.id)
                .then(changed)
                .catch(() =>
                  setError("Connection could not be canceled. Try again."),
                );
            }}
          >
            Cancel
          </button>
        </>
      ) : (
        <div className="actions">
          {connector.state.kind === "connected" ? (
            <>
              <button
                disabled={busy}
                onClick={() =>
                  void act(async () => {
                    await backend.testConnection(connector.id);
                    setNotice("Connection test passed");
                    await changed();
                  })
                }
              >
                Test
              </button>
              <button
                disabled={busy}
                onClick={() =>
                  void act(async () => {
                    await backend.disconnect(connector.id);
                    setNotice("Disconnected");
                    await changed();
                  })
                }
              >
                Disconnect
              </button>
            </>
          ) : (
            <button
              className="primary"
              disabled={
                busy ||
                !connector.mcpUrl ||
                (connector.registration === "preregistered" &&
                  !connector.hasClientCredentials)
              }
              onClick={() => void act(connect)}
            >
              Connect
            </button>
          )}
          {connector.state.kind !== "connected" && connector.registration === "preregistered" && !connector.hasClientCredentials && (
            <p className="muted">Connect turns on after you save a client ID above.</p>
          )}
        </div>
      )}
      {connector.state.kind === "error" && (
        <p role="alert">{connector.state.message}</p>
      )}
    </section>
  );
}

/** Email with an address and app password: one form, no browser sign-in. */
function MailDetail({ backend, connector, changed, focusTick }: {
  backend: Backend;
  connector: ConnectorInfo;
  changed: () => Promise<void>;
  focusTick: number;
}) {
  const mail = connector.mail!;
  const { act, busy, error } = useAction();
  const heading = useRef<HTMLHeadingElement>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [imapHost, setImapHost] = useState(mail.imapHost);
  const [imapPort, setImapPort] = useState(String(mail.imapPort));
  const [smtpHost, setSmtpHost] = useState(mail.smtpHost);
  const [smtpPort, setSmtpPort] = useState(String(mail.smtpPort));
  const [starttls, setStarttls] = useState(mail.smtpStarttls);
  const [confirmLink, setConfirmLink] = useState<string | null>(null);
  useEffect(() => {
    heading.current?.scrollIntoView?.({ block: "start", behavior: "smooth" });
    heading.current?.focus({ preventScroll: true });
  }, [focusTick]);
  const connected = connector.state.kind === "connected";
  return (
    <section className="connection-detail" aria-label={`${connector.name} connection details`}>
      <h3 ref={heading} tabIndex={-1}>{connector.name} connection</h3>
      <Notice error={error} />
      {notice && <p className="notice notice-success" role="status">{notice}</p>}
      {connected ? (
        <>
          <p>Signed in as <strong>{mail.email ?? "your mailbox"}</strong>. The password stays in your Mac's keychain.</p>
          <div className="actions">
            <button disabled={busy} onClick={() => void act(async () => { await backend.testConnection(connector.id); setNotice("Connection test passed"); await changed(); })}>Test</button>
            <button disabled={busy} onClick={() => void act(async () => { await backend.disconnect(connector.id); setNotice("Disconnected"); await changed(); })}>Disconnect</button>
          </div>
        </>
      ) : (
        <form onSubmit={(e) => {
          e.preventDefault();
          const pw = password;
          void act(async () => {
            const server = mail.customServer ? { imapHost, imapPort: Number(imapPort), smtpHost, smtpPort: Number(smtpPort), smtpStarttls: starttls } : null;
            const result = await backend.connectMail(connector.id, email, pw, server);
            setPassword("");
            setNotice(result.state.kind === "connected" ? `Connected · ${result.state.toolCount} tools available` : "Signed in. Check connection status.");
            await changed();
          });
        }}>
          <p>{mail.help}</p>
          <p className="muted">Nothing leaves your Mac except the sign-in to your mail provider. Every send, draft or change is shown to you for approval before it runs.</p>
          <Field label="Email address">
            <input type="email" required autoComplete="off" value={email} onChange={(e) => setEmail(e.target.value)} />
          </Field>
          <Field label="App password">
            <input type="password" required autoComplete="off" value={password} onChange={(e) => setPassword(e.target.value)} />
          </Field>
          {mail.customServer && (
            <>
              <Field label="IMAP server"><input required value={imapHost} onChange={(e) => setImapHost(e.target.value)} placeholder="imap.example.com" /></Field>
              <Field label="IMAP port"><input required inputMode="numeric" value={imapPort} onChange={(e) => setImapPort(e.target.value)} /></Field>
              <Field label="SMTP server"><input required value={smtpHost} onChange={(e) => setSmtpHost(e.target.value)} placeholder="smtp.example.com" /></Field>
              <Field label="SMTP port"><input required inputMode="numeric" value={smtpPort} onChange={(e) => setSmtpPort(e.target.value)} /></Field>
              <label className="recording-choice"><input type="checkbox" checked={starttls} onChange={(e) => setStarttls(e.target.checked)} />Start encryption after connecting (usually port 587)</label>
            </>
          )}
          <div className="actions">
            <button className="primary" disabled={busy || !email.trim() || !password}>Connect</button>
          </div>
          <p>
            {mail.appPasswordUrl && isHttpsUrl(mail.appPasswordUrl) && (
              <button type="button" onClick={() => setConfirmLink(mail.appPasswordUrl!)}>Open the app password page</button>
            )}{" "}
            {connector.docsUrl && isHttpsUrl(connector.docsUrl) && (
              <button type="button" onClick={() => setConfirmLink(connector.docsUrl!)}>Read the provider's instructions</button>
            )}
          </p>
        </form>
      )}
      {confirmLink && (
        <div className="notice notice-info" role="region" aria-label="Open link">
          <p>Open this address in your browser?</p>
          <p><code>{confirmLink}</code></p>
          <div className="actions">
            <button disabled={busy} onClick={() => void act(async () => { await backend.openExternal(confirmLink); setConfirmLink(null); })}>Open in browser</button>
            <button onClick={() => setConfirmLink(null)}>Keep here</button>
          </div>
        </div>
      )}
      {connector.state.kind === "error" && <p role="alert">{connector.state.message}</p>}
    </section>
  );
}

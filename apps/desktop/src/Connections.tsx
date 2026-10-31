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
}: {
  backend: Backend;
  connectors: ConnectorInfo[];
  selected: string | null;
  select: (id: string) => void;
  changed: () => Promise<void>;
  /** Start signing in to the selected service as soon as it opens (set when it was chosen elsewhere). */
  autoStart?: boolean;
}) {
  // Every click on a card, even the one already open, brings its details into view.
  const [tick, setTick] = useState(0);
  const [startTick, setStartTick] = useState(autoStart ? 1 : 0);
  return (
    <section>
      {connectors.length === 0 && <p role="status">Loading connections…</p>}
      <p>
        Connect a service for later tools. CSV tools do not use these
        connections.
      </p>
      <div className="connector-grid">
        {connectors.map((c) => (
          <article className={`card${selected === c.id ? " card-open" : ""}`} key={c.id}>
            <h2>{c.name}</h2>
            <p>{c.category.charAt(0).toUpperCase() + c.category.slice(1)}</p>
            <p className="badge">{stateName(c)}</p>
            <p className="badge">
              {c.verification === "verified-live" ||
              c.verification === "verified-docs"
                ? "Verified endpoint"
                : "Unverified endpoint"}
            </p>
            <ConnectorTools backend={backend} connectorId={c.id} />
            {c.registration === "api" && <>
              <p><bdi>{c.mcpUrl}</bdi></p>
              <p>One approved GET read. Credentials stay in the app's secret store.</p>
              {c.state.kind === "error" && <p role="alert">{c.state.message}</p>}
              <ApiConnectionActions backend={backend} connector={c} changed={changed} />
            </>}
            {selected === c.id && c.registration !== "api" && (
              <ConnectionDetail
                key={c.id}
                backend={backend}
                connector={c}
                changed={changed}
                focusTick={tick}
                startTick={startTick}
              />
            )}
            {c.registration !== "api" && <button
              aria-expanded={selected === c.id}
              onClick={() => {
                select(c.id);
                setTick((t) => t + 1);
                // Choosing a service that isn't connected starts its sign-in when it can.
                if (c.state.kind !== "connected") setStartTick((t) => t + 1);
              }}
            >
              {c.state.kind === "connected"
                ? `Manage ${c.name}`
                : `Connect ${c.name}`}
            </button>}
          </article>
        ))}
      </div>
      <details><summary>Connect a custom API</summary><ApiSetup backend={backend} changed={changed} /></details>
    </section>
  );
}
function ApiConnectionActions({ backend, connector, changed }: { backend: Backend; connector: ConnectorInfo; changed: () => Promise<void> }) {
  const { act, busy, error } = useAction();
  return <>
    <button disabled={busy} onClick={() => void act(async () => { try { await backend.testConnection(connector.id); } finally { await changed(); } })}>Test API connection</button>
    <button disabled={busy} onClick={() => void act(async () => { await backend.disconnect(connector.id); await changed(); })}>Remove API connection</button>
    {error && <p role="alert">{error}</p>}
  </>;
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
      {busy && <p role="status">Loading tools…</p>}
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
      {pending ? (
        <>
          <p role="status">Finish signing in in your browser…</p>
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

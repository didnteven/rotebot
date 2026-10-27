import { useRef, useState } from "react";
import type {
  Backend,
  ConnectStart,
  ConnectorInfo,
  ConnectorTool,
} from "./backend/types";
import { Field, isHttpsUrl, Notice, useAction } from "./ui";

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
}: {
  backend: Backend;
  connectors: ConnectorInfo[];
  selected: string | null;
  select: (id: string) => void;
  changed: () => Promise<void>;
}) {
  const connector = connectors.find((c) => c.id === selected);
  return (
    <section>
      {connectors.length === 0 && <p role="status">Loading connections…</p>}
      <p>
        Connect a service for later tools. CSV tools do not use these
        connections.
      </p>
      <div className="connector-grid">
        {connectors.map((c) => (
          <article className="card" key={c.id}>
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
            <button
              aria-expanded={selected === c.id}
              onClick={() => select(c.id)}
            >
              {c.state.kind === "connected"
                ? `Manage ${c.name}`
                : `Connect ${c.name}`}
            </button>
          </article>
        ))}
      </div>
      {connector && (
        <ConnectionDetail
          key={connector.id}
          backend={backend}
          connector={connector}
          changed={changed}
        />
      )}
    </section>
  );
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
}: {
  backend: Backend;
  connector: ConnectorInfo;
  changed: () => Promise<void>;
}) {
  const [url, setUrl] = useState(connector.mcpUrl),
    [confirmDocs, setConfirmDocs] = useState(false),
    [pending, setPending] = useState<ConnectStart | null>(null),
    [notice, setNotice] = useState<string | null>(null);
  const clientId = useRef<HTMLInputElement>(null),
    clientSecret = useRef<HTMLInputElement>(null),
    attempt = useRef(0);
  const { act, busy, error, setError } = useAction();
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
  return (
    <section
      className="card connection-detail"
      aria-label={`${connector.name} connection details`}
    >
      <h2>{connector.name} connection</h2>
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
              await backend.setClientCredentials(
                connector.id,
                id,
                secret || null,
              );
              setNotice("Client credentials saved");
              await changed();
            });
          }}
        >
          {connector.clientSetup && <p>{connector.clientSetup}</p>}
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
              : "Client ID required before Connect."}
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
        </div>
      )}
      {connector.state.kind === "error" && (
        <p role="alert">{connector.state.message}</p>
      )}
    </section>
  );
}

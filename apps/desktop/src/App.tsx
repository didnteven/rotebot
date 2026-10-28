import { useEffect, useState } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type {
  AppInfo,
  Backend,
  ConnectionSuggestion,
  ConnectorInfo,
  CsvPreview,
  ModelAccessStatus,
  ShowState,
  ToolSummary,
} from "./backend/types";
import { routeLabel } from "./backend/mock";
import { CreateFlow } from "./CreateFlow";
import { Today } from "./Today";
import { RunFlow } from "./RunFlow";
import { Settings } from "./Settings";
import { Connections } from "./Connections";
import { Activity, Privacy } from "./History";
import { RecordingBar, ShowPanel, ShowStatus } from "./Show";
import { FileText, Notice, Suggestions, useAction } from "./ui";

const destinations = [
  "Today",
  "Tools",
  "Activity",
  "Connections",
  "Privacy",
  "Settings",
] as const;
type Destination = (typeof destinations)[number];
export function App({ backend }: { backend: Backend }) {
  const [destination, setDestination] = useState<Destination>("Tools");
  const [tools, setTools] = useState<ToolSummary[] | null>(null),
    [libraryFailed, setLibraryFailed] = useState(false);
  const [status, setStatus] = useState<ModelAccessStatus | null>(null),
    [info, setInfo] = useState<AppInfo | null>(null),
    [show, setShow] = useState<ShowState | null>(null);
  const [connectors, setConnectors] = useState<ConnectorInfo[]>([]),
    [suggestions, setSuggestions] = useState<ConnectionSuggestion[]>([]),
    [selectedConnector, setSelectedConnector] = useState<string | null>(null);
  const [creating, setCreating] = useState(false),
    [initialInput, setInitialInput] = useState<CsvPreview | null>(null),
    [running, setRunning] = useState<ToolSummary | null>(null),
    [showPanel, setShowPanel] = useState(false);
  const { act, error, busy } = useAction();
  async function refreshTools() {
    try {
      setTools(await backend.listTools());
      setLibraryFailed(false);
    } catch {
      setLibraryFailed(true);
    }
  }
  async function refreshConnections() {
    const [c, s] = await Promise.all([
      backend.listConnectors(),
      backend.connectionSuggestions(),
    ]);
    setConnectors(c);
    setSuggestions(s);
  }
  async function refresh() {
    const [s, i, sh] = await Promise.all([
      backend.modelAccess(),
      backend.appInfo(),
      backend.showState(),
    ]);
    setStatus(s);
    setInfo(i);
    setShow(sh);
    await refreshConnections();
  }
  useEffect(() => {
    let active = true;
    let unsubscribe: (() => void) | undefined;
    void refreshTools();
    void act(refresh);
    void backend
      .onShowEvent((state) => {
        if (active) {
          setShow(state);
          void backend
            .connectionSuggestions()
            .then((s) => {
              if (active) setSuggestions(s);
            })
            .catch(() => {});
        }
      })
      .then((fn) => {
        if (active) unsubscribe = fn;
        else fn();
      })
      .catch(() => {});
    return () => {
      active = false;
      unsubscribe?.();
    };
  }, [backend]);
  useEffect(() => {
    if (!isTauri()) return;
    let active = true;
    const subscriptions = [
      listen<string>("navigate://run", (event) => {
        void act(async () => {
          const tool = await backend.getTool(event.payload);
          if (active) {
            setCreating(false);
            setShowPanel(false);
            setRunning(tool);
            setDestination("Tools");
          }
        });
      }),
      listen("navigate://today", () => { if (active) setDestination("Today"); }),
      listen("navigate://create", () => {
        if (active) {
          setRunning(null);
          setShowPanel(false);
          setInitialInput(null);
          setCreating(true);
          setDestination("Tools");
        }
      }),
    ];
    subscriptions.forEach((p) => void p.catch(() => {}));
    return () => {
      active = false;
      subscriptions.forEach(
        (p) => void p.then((stop) => stop()).catch(() => {}),
      );
    };
  }, [backend]);
  const [exported, setExported] = useState<string | null>(null);
  function connect(id: string) {
    setSelectedConnector(id);
    setDestination("Connections");
  }
  const openSettings = () => setDestination("Settings");
  const updateShow = (state: ShowState) => {
    setShow(state);
    void act(refreshConnections);
  };
  const commonSuggestions = (
    <Suggestions
      suggestions={suggestions}
      connect={connect}
      dismiss={(id) =>
        void act(async () => {
          await backend.dismissSuggestion(id);
          await refreshConnections();
        })
      }
    />
  );
  return (
    <div className="app-shell">
      <aside className="sidebar">
        <div className="brand">Macroscope</div>
        <nav aria-label="Main navigation">
          {destinations.map((item) => (
            <button
              key={item}
              aria-current={destination === item ? "page" : undefined}
              onClick={() => setDestination(item)}
            >
              {item}
            </button>
          ))}
        </nav>
        <p className="sidebar-note">Small tools for everyday work.</p>
      </aside>
      <div className="workspace">
        {show?.kind === "recording" && (
          <RecordingBar backend={backend} state={show} update={updateShow} />
        )}
        <div className="status-strip" role="region" aria-label="App status">
          <span>
            Building with:{" "}
            {status ? `${routeLabel(status.effectiveRoute)}${status.effectiveRoute === "custom_endpoint" ? ` · ${status.customEndpoint.model?.split("/").pop()} · ${status.customEndpoint.host}` : ""}` : "Checking…"}
          </span>
          <span>
            Cloud models:{" "}
            {status?.egress === "stopping"
              ? "Stopping…"
              : status?.egress === "local_only"
                ? "Off (local only)"
                : status?.egress === "key_unavailable"
                  ? "Key unavailable"
                  : status
                    ? "On"
                    : "Checking…"}
          </span>
          <ShowStatus state={show} />
          {show && show.kind !== "unavailable" && (
            <button
              onClick={() => {
                setDestination("Tools");
                setShowPanel(true);
              }}
            >
              Show a task
            </button>
          )}
        </div>
        <main id="main-content" aria-labelledby="page-title">
          <header>
            <h1 id="page-title">{destination}</h1>
          </header>
          <Notice error={error} />
          {exported && <p role="status">Tool exported to <FileText text={exported} /></p>}
          {error && (
            <button onClick={() => void act(refresh)}>Retry app status</button>
          )}
          {!status && !error && <p role="status">Loading Macroscope…</p>}
          {destination === "Today" && <Today backend={backend} tools={() => setDestination("Tools")} />}
          <div hidden={destination !== "Tools"}>
            {destination === "Tools" && commonSuggestions}
            {showPanel && show && (
              <ShowPanel
                backend={backend}
                state={show}
                update={updateShow}
                close={() => setShowPanel(false)}
              />
            )}
            <div hidden={showPanel}>
              {creating && status ? (
                <CreateFlow
                  backend={backend}
                  initialInput={initialInput}
                  status={status}
                  connectors={connectors}
                  openSettings={openSettings}
                  connect={connect}
                  close={() => setCreating(false)}
                  saved={async () => {
                    await refreshTools();
                    setCreating(false);
                  }}
                />
              ) : running ? (
                <RunFlow
                  backend={backend}
                  tool={running}
                  close={() => setRunning(null)}
                  changed={refreshTools}
                  openSettings={openSettings}
                />
              ) : libraryFailed ? (
                <div role="alert">
                  <p>Tools could not be loaded. Try reopening Macroscope.</p>
                  <button onClick={() => void refreshTools()}>
                    Reload tools
                  </button>
                </div>
              ) : tools === null ? (
                <p role="status">Loading tools…</p>
              ) : (
                <>
                  {tools.length === 0 ? (
                    <section className="empty-state">
                      <div className="empty-symbol" aria-hidden="true">
                        ✳
                      </div>
                      <h2>
                        Turn a repetitive task into a tool you can test and
                        keep.
                      </h2>
                      <div className="actions">
                        <button
                          className="primary"
                          disabled={busy || !status}
                          onClick={() => {
                            setInitialInput(null);
                            setCreating(true);
                          }}
                        >
                          Describe a task
                        </button>
                        <button
                          disabled={busy || !status}
                          onClick={() =>
                            void act(async () => {
                              setInitialInput(await backend.loadSampleCsv());
                              setCreating(true);
                            })
                          }
                        >
                          Try a sample
                        </button>
                      </div>
                      <div className="sample-line">
                        <strong>Sample: messy CSV → clean import</strong>
                        <p>
                          Input: date=10/22/26, amount=$1,240.00
                          <br />
                          Output: date=2026-10-22, amount=1240.00
                        </p>
                      </div>
                      <p>
                        Selected data reaches Meta only after an outbound
                        review. No observation is needed to make this tool.
                      </p>
                    </section>
                  ) : (
                    <>
                      <div className="actions">
                        <button
                          className="primary"
                          onClick={() => {
                            setInitialInput(null);
                            setCreating(true);
                          }}
                        >
                          Describe a task
                        </button>
                      </div>
                      <ul className="plain-list">
                        {tools.map((tool) => (
                          <li key={tool.id} className="tool-row">
                            <div>
                              <h2>
                                {tool.name}{" "}
                                {tool.sample && (
                                  <span className="badge">Sample</span>
                                )}
                              </h2>
                              <p>
                                {tool.engine === "js-quickjs" ? "Program (sandboxed) · " : ""}Version {tool.version} · Last run:{" "}
                                {tool.lastRunAtMs
                                  ? new Date(tool.lastRunAtMs).toLocaleString()
                                  : "Never"}
                              </p>
                            </div>
                            <button
                              disabled={busy}
                              onClick={() =>
                                void act(async () => {
                                  setExported(
                                    await backend.exportTool(tool.id),
                                  );
                                })
                              }
                            >
                              Export…
                            </button>
                            <button
                              disabled={busy}
                              onClick={() =>
                                void act(async () => {
                                  if (tool.state === "paused")
                                    await backend.restoreTool(tool.id);
                                  else await backend.revokeTool(tool.id);
                                  await refreshTools();
                                })
                              }
                            >
                              {tool.state === "paused"
                                ? "Resume tool"
                                : "Pause tool"}
                            </button>
                            <button
                              disabled={tool.state === "paused"}
                              aria-label={`Run ${tool.name}…`}
                              onClick={() => setRunning(tool)}
                            >
                              Run…
                            </button>
                          </li>
                        ))}
                      </ul>
                    </>
                  )}
                </>
              )}
            </div>
          </div>
          {destination === "Activity" && <Activity backend={backend} />}
          {destination === "Connections" && (
            <>
              {commonSuggestions}
              <Connections
                backend={backend}
                connectors={connectors}
                selected={selectedConnector}
                select={setSelectedConnector}
                changed={refreshConnections}
              />
            </>
          )}
          {destination === "Privacy" && status && (
            <Privacy
              backend={backend}
              status={status}
              update={setStatus}
              stopped={async () => {
                setCreating(false);
                setRunning(null);
                setInitialInput(null);
                await refresh();
                await refreshTools();
              }}
            />
          )}
          {destination === "Settings" && status && info && show && (
            <Settings
              backend={backend}
              status={status}
              info={info}
              show={show}
              update={setStatus}
            />
          )}
        </main>
      </div>
    </div>
  );
}

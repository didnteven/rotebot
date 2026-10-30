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
  ToolDetail,
  ToolSummary,
} from "./backend/types";
import { routeLabel } from "./backend/mock";
import { CreateFlow } from "./CreateFlow";
import { Today } from "./Today";
import { RunFlow } from "./RunFlow";
import { ToolVersions } from "./ToolVersions";
import { Settings } from "./Settings";
import { Connections } from "./Connections";
import { Activity, Privacy } from "./History";
import { RecordingBar, ShowPanel } from "./Show";
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
  const [confirmDelete, setConfirmDelete] = useState<string | null>(null);
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
    [editTool, setEditTool] = useState<ToolDetail | null>(null),
    [showPanel, setShowPanel] = useState(false);
  const { act, error, busy } = useAction();
  // Leaving the create flow, by saving or going back, ends an edit.
  useEffect(() => { if (!creating) setEditTool(null); }, [creating]);
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
  const [toolDescriptions, setToolDescriptions] = useState<Record<string, string>>({});
  useEffect(() => {
    let active = true;
    void Promise.all((tools ?? []).map(async tool => {
      try {
        const detail = await backend.getTool(tool.id);
        const summary = detail.summary?.find(line => line.trim() && !line.includes("recurrence unmeasured") && !line.startsWith("Program (sandboxed)"));
        return [tool.id, summary || detail.program?.summary || tool.name];
      }
      catch { return [tool.id, tool.name]; }
    })).then(entries => { if (active) setToolDescriptions(Object.fromEntries(entries)); });
    return () => { active = false; };
  }, [backend, tools]);
  const [exported, setExported] = useState<string | null>(null);
  // Asked for from a plan or a suggestion: open the service and start its sign-in when it can.
  const [autoConnect, setAutoConnect] = useState(false);
  useEffect(() => { if (autoConnect && destination === "Connections") setAutoConnect(false); }, [autoConnect, destination]);
  function connect(id: string) {
    setSelectedConnector(id);
    setAutoConnect(true);
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
      <a className="skip-link" href="#main-content">Skip to main content</a>
      <aside className="sidebar">
        <div className="brand">Macroscope</div>
        <nav aria-label="Main navigation">
          {destinations.map((item, index) => (
            <button
              className={index === 3 ? "secondary-start" : index > 3 ? "secondary-nav" : undefined}
              key={item}
              aria-current={destination === item ? "page" : undefined}
              onClick={() => setDestination(item)}
            >
              {item === "Tools" ? "My tools" : item}
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
          <span>{status ? `Using ${routeLabel(status.effectiveRoute)} - cloud models ${status.egress === "cloud_allowed" ? "on" : status.egress === "stopping" ? "stopping" : status.egress === "key_unavailable" ? "unavailable (check your key)" : "off (local only)"}` : "Checking model access…"}</span>
          <button className="quiet" aria-label="Settings for model access" onClick={openSettings}>Settings</button>
          {show && (
            <button
              onClick={() => {
                setDestination("Tools");
                setShowPanel(true);
              }}
            >
              Record a task
            </button>
          )}
        </div>
        <main id="main-content" tabIndex={-1} aria-labelledby="page-title">
          <header>
            <h1 id="page-title">{destination === "Tools" ? "My tools" : destination}</h1>
          </header>
          <Notice error={error} />
          {exported && <p role="status">Tool exported to <FileText text={exported} /></p>}
          {error && (
            <button onClick={() => void act(refresh)}>Retry app status</button>
          )}
          {!status && !error && <p role="status">Loading Macroscope…</p>}
          {destination === "Today" && <Today backend={backend} tools={() => setDestination("Tools")} run={id => void act(async () => { const tool = await backend.getTool(id); setCreating(false); setShowPanel(false); setRunning(tool); setDestination("Tools"); })} create={() => { setDestination("Tools"); setInitialInput(null); setCreating(true); }} />}
          <div hidden={destination !== "Tools"}>
            {destination === "Tools" && !creating && commonSuggestions}
            {showPanel && show && status && (
              <ShowPanel
                backend={backend}
                state={show}
                createFlow={{status, connectors, openSettings, connect, close: () => setShowPanel(false), saved: async () => {await refreshTools();setShowPanel(false);}}}
                update={updateShow}
                close={() => setShowPanel(false)}
              />
            )}
            <div hidden={showPanel}>
              {creating && status ? (
                <CreateFlow
                  backend={backend}
                  initialInput={initialInput}
                  editTool={editTool ?? undefined}
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
                        You choose what to send before building. Check the result before saving your tool.
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
                        {tools.map((tool, index) => {
                          const duplicate = tools.filter(t => t.name === tool.name).length > 1;
                          const displayName = duplicate ? `${tool.name} (${tools.slice(0, index + 1).filter(t => t.name === tool.name).length})` : tool.name;
                          return (
                          <li key={tool.id} className="tool-row">
                            <div>
                              <h2>
                                {displayName}{" "}
                                {tool.sample && (
                                  <span className="badge">Sample</span>
                                )}
                              </h2>
                              <p>{toolDescriptions[tool.id] || tool.name}</p>
                              <p className="muted">{friendlyLastRun(tool.lastRunAtMs)}{tool.state === "paused" ? " · Paused. Resume this tool before running it." : ""}</p>
                              <details><summary>Details</summary><p>{tool.engine === "js-quickjs" ? "Program (sandboxed) · " : ""}Version {tool.version}</p>{!tool.sample && <ToolVersions backend={backend} toolId={tool.id} name={displayName} changed={refreshTools} />}</details>
                            </div>
                            <button
                              className="quiet"
                              disabled={busy}
                              onClick={() =>
                                void act(async () => {
                                  setExported(
                                    await backend.exportTool(tool.id),
                                  );
                                })
                              }
                            >
                              Export
                            </button>
                            <button
                              className="quiet"
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
                            {confirmDelete === tool.id ? (
                              <div role="alertdialog" aria-label={`Delete ${displayName}`}>
                                <p>Delete “{displayName}”? Its runs and costs are removed. Files it already created stay where they are.</p>
                                <button disabled={busy} onClick={() => void act(async () => {
                                  await backend.deleteTool(tool.id);
                                  setConfirmDelete(null);
                                  await refreshTools();
                                })}>Delete tool</button>
                                <button className="quiet" onClick={() => setConfirmDelete(null)}>Keep tool</button>
                              </div>
                            ) : (
                              <button className="quiet" disabled={busy} aria-label={`Delete: ${displayName}`} onClick={() => setConfirmDelete(tool.id)}>Delete</button>
                            )}
                            {!tool.sample && <button
                              className="quiet"
                              disabled={busy}
                              aria-label={`Edit: ${displayName}`}
                              onClick={() => void act(async () => {
                                setEditTool(await backend.getTool(tool.id));
                                setInitialInput(null);
                                setRunning(null);
                                setCreating(true);
                              })}
                            >
                              Edit
                            </button>}
                            <button
                              disabled={tool.state === "paused"}
                              aria-label={`Run now: ${displayName}`}
                              onClick={() => setRunning(tool)}
                            >
                              Run now
                            </button>
                          </li>
                        ); })}
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
                autoStart={autoConnect}
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

function friendlyLastRun(at: number | null) {
  if (at === null) return "Never run";
  const date = new Date(at), now = new Date();
  const time = date.toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });
  if (date.toDateString() === now.toDateString()) return `Ran today at ${time}`;
  const yesterday = new Date(now); yesterday.setDate(now.getDate() - 1);
  if (date.toDateString() === yesterday.toDateString()) return `Ran yesterday at ${time}`;
  return `Ran on ${date.toLocaleDateString([], { month: "short", day: "numeric", year: "numeric" })} at ${time}`;
}

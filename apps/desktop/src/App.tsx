import { BotLibraryProvider } from "./BotLibrary";
import { CustomBotStudio } from "./CustomBotStudio";
import { BotAvatar, BotStatus, botIdentity, saveBotIdentity } from "./Bot";
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
  Schedule,
  ShowState,
  ToolDetail,
  ToolSummary,
} from "./backend/types";
import { CreateFlow } from "./CreateFlow";
import { Today } from "./Today";
import { RunFlow } from "./RunFlow";
import { nextRunText, whenText } from "./Schedule";
import { ToolVersions } from "./ToolVersions";
import { Settings } from "./Settings";
import { Connections } from "./Connections";
import { Activity, Privacy } from "./History";
import { RecordingBar, ShowPanel } from "./Show";
import { FileText, Notice, Suggestions, useAction } from "./ui";

type Destination = "Today" | "Tools" | "Activity" | "Connections" | "Privacy" | "Settings";
/** Privacy lives under Settings, so the sidebar stays short. */
const navigation: { item: Destination; label: string; secondary?: boolean }[] = [
  { item: "Today", label: "Today" },
  { item: "Tools", label: "My tools" },
  { item: "Activity", label: "Activity" },
  { item: "Connections", label: "Connections", secondary: true },
  { item: "Settings", label: "Settings", secondary: true },
];
export function App({ backend }: { backend: Backend }) {
  return <BotLibraryProvider backend={backend}><AppWorkspace backend={backend} /></BotLibraryProvider>;
}
function AppWorkspace({ backend }: { backend: Backend }) {
  const [destination, setDestination] = useState<Destination>("Tools");
  const [customizingTool, setCustomizingTool] = useState<ToolSummary | null>(null);
  const [confirmDelete, setConfirmDelete] = useState<string | null>(null);
  const [menuOpen, setMenuOpen] = useState<string | null>(null);
  const [expanded, setExpanded] = useState<string | null>(null);
  const [renaming, setRenaming] = useState<{ id: string; name: string } | null>(null);
  // An open ⋯ menu closes on a click outside it or on Escape, like any menu.
  useEffect(() => {
    if (!menuOpen) return;
    const close = (event: Event) => {
      if (event instanceof KeyboardEvent ? event.key === "Escape" : !(event.target as Element | null)?.closest?.(".more-menu")) setMenuOpen(null);
    };
    document.addEventListener("mousedown", close);
    document.addEventListener("keydown", close);
    return () => { document.removeEventListener("mousedown", close); document.removeEventListener("keydown", close); };
  }, [menuOpen]);
  const [tools, setTools] = useState<ToolSummary[] | null>(null),
    [libraryFailed, setLibraryFailed] = useState(false);
  const [status, setStatus] = useState<ModelAccessStatus | null>(null),
    [info, setInfo] = useState<AppInfo | null>(null),
    [show, setShow] = useState<ShowState | null>(null);
  const [connectionsLoaded, setConnectionsLoaded] = useState(false);
  const [connectors, setConnectors] = useState<ConnectorInfo[]>([]),
    [suggestions, setSuggestions] = useState<ConnectionSuggestion[]>([]),
    [selectedConnector, setSelectedConnector] = useState<string | null>(null);
  const [creating, setCreating] = useState(false),
    [initialInput, setInitialInput] = useState<CsvPreview | null>(null),
    [running, setRunning] = useState<ToolSummary | null>(null),
    [editTool, setEditTool] = useState<ToolDetail | null>(null),
    [scheduleFirst, setScheduleFirst] = useState(false),
    [schedules, setSchedules] = useState<Schedule[]>([]),
    [showPanel, setShowPanel] = useState(false);
  const { act, error, busy } = useAction();
  // Leaving the create flow, by saving or going back, ends an edit.
  useEffect(() => { if (!creating) setEditTool(null); }, [creating]);
  async function refreshTools() {
    try {
      setTools(await backend.listTools());
      setLibraryFailed(false);
      // The schedule line on each card is a convenience; the library still works without it.
      setSchedules(await backend.listSchedules().catch(() => []));
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
    setConnectionsLoaded(true);
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
  /** What each saved tool uses: the exact connections it reads or acts through, and the web sites it reads. */
  const [toolUses, setToolUses] = useState<Record<string, { connections: string[]; sites: string[] }>>({});
  useEffect(() => {
    let active = true;
    void Promise.all((tools ?? []).map(async tool => {
      try {
        const detail = await backend.getTool(tool.id);
        const summary = detail.summary?.find(line => line.trim() && !line.includes("recurrence unmeasured") && !line.startsWith("Program (sandboxed)"));
        const code = detail.program?.code ?? "";
        // Reads are declared; actions name their connection in the code (macroscope.connection.request("id", …)).
        const ids = [...new Set([...(detail.program?.connectionSources ?? []).map(s => s.connectorId),
          ...[...code.matchAll(/connection\.request\(\s*["']([^"']+)["']/g)].map(m => m[1])])];
        const sites = [...new Set((detail.program?.webSources ?? []).map(w => { try { return new URL(w.url).hostname; } catch { return ""; } }).filter(Boolean))];
        return [tool.id, summary || detail.program?.summary || tool.name, { connections: ids, sites }] as const;
      }
      catch { return [tool.id, tool.name, { connections: [] as string[], sites: [] as string[] }] as const; }
    })).then(entries => { if (active) { setToolDescriptions(Object.fromEntries(entries.map(([id, d]) => [id, d]))); setToolUses(Object.fromEntries(entries.map(([id, , u]) => [id, u]))); } });
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
  const aiReady = !!status && status.egress === "cloud_allowed" && !!status.effectiveRoute && status.effectiveRoute !== "offline_demo";
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
      {customizingTool && status && <CustomBotStudio backend={backend} status={status} currentSeed={botIdentity(customizingTool.id)} title={customizingTool.name} useBot={seed => saveBotIdentity(customizingTool.id, seed)} close={() => setCustomizingTool(null)} openSettings={openSettings} />}
      <a className="skip-link" href="#main-content">Skip to main content</a>
      <aside className="sidebar">
        <div className="brand"><BotAvatar size={36} />Macroscope</div>
        <nav aria-label="Main navigation">
          {navigation.map(({ item, label, secondary }, index) => (
            <button
              className={secondary ? (navigation[index - 1]?.secondary ? "secondary-nav" : "secondary-start") : undefined}
              key={item}
              aria-current={destination === item || (item === "Settings" && destination === "Privacy") ? "page" : undefined}
              onClick={() => setDestination(item)}
            >
              {label}
              {item === "Tools" && destination !== "Tools" && (creating || running || showPanel) && <span className="nav-dot" aria-hidden="true" title="Task in progress" />}
            </button>
          ))}
        </nav>
      </aside>
      <div className="workspace">
        {show?.kind === "recording" && (
          <RecordingBar backend={backend} state={show} update={updateShow} />
        )}
        {status && (status.egress === "key_unavailable" || status.effectiveRoute === null) && destination !== "Settings" && (
          <div className="status-strip notice-warning" role="region" aria-label="App status">
            <span>{status.egress === "key_unavailable" ? "Your AI service isn’t working. Check your key." : "No AI service is set up."}</span>
            <button className="ghost" onClick={openSettings}>Fix in Settings</button>
          </div>
        )}
        <main id="main-content" tabIndex={-1} aria-labelledby="page-title">
          <header>
            <h1 id="page-title">{destination === "Tools" ? "My tools" : destination === "Privacy" ? "Settings" : destination}</h1>
          </header>
          {(destination === "Settings" || destination === "Privacy") && <div className="tabs" role="tablist" aria-label="Settings sections">
            <button role="tab" aria-selected={destination === "Settings"} onClick={() => setDestination("Settings")}>AI service</button>
            <button role="tab" aria-selected={destination === "Privacy"} onClick={() => setDestination("Privacy")}>Privacy &amp; data</button>
          </div>}
          <Notice error={error} />
          {exported && <p role="status">Tool exported to <FileText text={exported} /></p>}
          {error && (
            <button onClick={() => void act(refresh)}>Retry app status</button>
          )}
          {!status && !error && <BotStatus label="Loading Macroscope…" />}
          {destination === "Today" && <Today backend={backend} tools={() => setDestination("Tools")} run={id => void act(async () => { const tool = await backend.getTool(id); setCreating(false); setShowPanel(false); setRunning(tool); setDestination("Tools"); })} create={() => { setDestination("Tools"); setInitialInput(null); setCreating(true); }} />}
          <div hidden={destination !== "Tools"}>
            {destination === "Tools" && !creating && commonSuggestions}
            {showPanel && show && status && (
              <ShowPanel
                backend={backend}
                state={show}
                createFlow={{status, connectors, connectionsChanged: refreshConnections, openSettings, connect, close: () => setShowPanel(false), saved: async () => {await refreshTools();setShowPanel(false);}}}
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
                  connectionsChanged={refreshConnections}
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
                  scheduleFirst={scheduleFirst}
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
                <BotStatus label="Loading tools…" />
              ) : (
                <>
                  {tools.length === 0 ? (
                    <section className="empty-state">
                      <BotAvatar size={144} />
                      <h2>Turn a repetitive task into a tool.</h2>
                      {status && !aiReady && <div className="notice notice-info"><p>First, connect an AI service so Macroscope can build your tools.</p><button className="primary" onClick={openSettings}>Connect an AI service</button></div>}
                      <div className="actions">
                        <button
                          className={aiReady ? "primary" : undefined}
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
                        {show && <button onClick={() => setShowPanel(true)}>Record a task</button>}
                      </div>
                    </section>
                  ) : (
                    <>
                      <div className="actions">
                        <button
                          onClick={() => {
                            setInitialInput(null);
                            setCreating(true);
                          }}
                        >
                          Describe a task
                        </button>
                        {show && <button className="quiet" onClick={() => setShowPanel(true)}>Record a task</button>}
                      </div>
                      <ul className="plain-list">
                        {tools.map((tool, index) => {
                          const duplicate = tools.filter(t => t.name === tool.name).length > 1;
                          const displayName = duplicate ? `${tool.name} (${tools.slice(0, index + 1).filter(t => t.name === tool.name).length})` : tool.name;
                          return (
                          <li key={tool.id} className="tool-row">
                            <BotAvatar seed={botIdentity(tool.id)} state={tool.state === "paused" ? "paused" : "idle"} size={64} />
                            <div className="tool-text">
                              {renaming?.id === tool.id ? (
                                <form className="rename-form" onSubmit={e => { e.preventDefault(); const name = renaming.name; void act(async () => { await backend.renameTool(tool.id, name); setRenaming(null); await refreshTools(); }); }}>
                                  <input aria-label={`New name for ${displayName}`} autoFocus value={renaming.name} maxLength={80} onChange={e => setRenaming({ id: tool.id, name: e.target.value })} onKeyDown={e => { if (e.key === "Escape") setRenaming(null); }} />
                                  <button className="primary" type="submit" disabled={busy || !renaming.name.trim()}>Save name</button>
                                  <button type="button" className="ghost" onClick={() => setRenaming(null)}>Cancel</button>
                                </form>
                              ) : <h2>
                                {displayName}{" "}
                                {tool.sample && (
                                  <span className="badge">Sample</span>
                                )}
                                {tool.state === "paused" && <span className="badge">Paused</span>}
                              </h2>}
                              {(() => {
                                const description = toolDescriptions[tool.id] || "";
                                // One line with a chevron for the rest; nothing when it only repeats the name.
                                if (!description || description === tool.name) return null;
                                const open = expanded === tool.id;
                                return <div className="tool-desc">
                                  <p className={open ? undefined : "clamp"}>{description}</p>
                                  {description.length > 60 && <button className="chevron" aria-expanded={open} aria-label={`${open ? "Hide" : "Show"} full description of ${displayName}`} onClick={() => setExpanded(open ? null : tool.id)}>{open ? "▴" : "▾"}</button>}
                                </div>;
                              })()}
                              {(() => {
                                const uses = toolUses[tool.id];
                                if (!uses || (!uses.connections.length && !uses.sites.length)) return null;
                                return <p className="tool-uses muted">Uses {[...uses.connections.map(id => {
                                  const c = connectors.find(x => x.id === id);
                                  return !c ? <span key={id} className="tool-use-missing">a removed connection</span>
                                    : c.state.kind !== "connected" ? <span key={id} className="tool-use-missing">{c.name} (not connected)</span>
                                    : <span key={id}>{c.name}</span>;
                                }), ...uses.sites.map(h => <span key={h}>{h}</span>)].flatMap((el, i) => i ? [<span key={`sep${i}`}> · </span>, el] : [el])}</p>;
                              })()}
                              <p className="muted">{(() => { const line = scheduleLine(schedules.find(s => s.toolId === tool.id)); return line ? <span data-testid="schedule-line">{line}</span> : friendlyLastRun(tool.lastRunAtMs); })()}</p>
                            </div>
                            <div className="tool-actions">
                              {tool.state === "paused" ? (
                                <button className="primary" disabled={busy} onClick={() => void act(async () => { await backend.restoreTool(tool.id); await refreshTools(); })}>Resume tool</button>
                              ) : (
                                <button
                                  className="primary"
                                  aria-label={`Run now: ${displayName}`}
                                  onClick={() => { setScheduleFirst(false); setRunning(tool); }}
                                >
                                  Run
                                </button>
                              )}
                              {!tool.sample && tool.state !== "paused" && <button
                                aria-label={`Schedule: ${displayName}`}
                                onClick={() => { setScheduleFirst(true); setRunning(tool); }}
                              >
                                {schedules.some(s => s.toolId === tool.id) ? "Edit schedule" : "Schedule"}
                              </button>}
                              <details className="more-menu" open={menuOpen === tool.id} onToggle={e => { const open = e.currentTarget.open; setMenuOpen(current => open ? tool.id : current === tool.id ? null : current); }}>
                                <summary aria-label={`More actions: ${displayName}`}>⋯</summary>
                                <div className="menu">
                                  {!tool.sample && <button
                                    className="quiet"
                                    disabled={busy}
                                    aria-label={`Edit: ${displayName}`}
                                    onClick={() => void act(async () => {
                                      setEditTool(await backend.getTool(tool.id));
                                      setInitialInput(null);
                                      setRunning(null);
                                      setMenuOpen(null);
                                      setCreating(true);
                                    })}
                                  >
                                    Edit
                                  </button>}
                                  <button className="quiet" disabled={busy} aria-label={`Rename: ${displayName}`} onClick={() => { setMenuOpen(null); setRenaming({ id: tool.id, name: tool.name.replace(/…$/, "") }); }}>Rename</button>
                                  <button className="quiet" disabled={busy || !status} aria-label={`Customize bot: ${displayName}`} onClick={() => { setMenuOpen(null); setCustomizingTool(tool); }}>Customize bot</button>
                                  <button className="quiet" disabled={busy} onClick={() => void act(async () => { setExported(await backend.exportTool(tool.id)); setMenuOpen(null); })}>Export</button>
                                  {tool.state !== "paused" && <button className="quiet" disabled={busy} onClick={() => void act(async () => { await backend.revokeTool(tool.id); setMenuOpen(null); await refreshTools(); })}>Pause tool</button>}
                                  {!tool.sample && <ToolVersions backend={backend} toolId={tool.id} name={displayName} changed={refreshTools} />}
                                  <button className="quiet danger" disabled={busy} aria-label={`Delete: ${displayName}`} onClick={() => { setMenuOpen(null); setConfirmDelete(tool.id); }}>Delete</button>
                                </div>
                              </details>
                            </div>
                            {confirmDelete === tool.id && (
                              <div role="alertdialog" aria-label={`Delete ${displayName}`} className="notice notice-warning confirm-row">
                                <p>Delete “{displayName}”? Files it already created are kept.</p>
                                <div className="actions">
                                  <button className="danger" disabled={busy} onClick={() => void act(async () => {
                                    await backend.deleteTool(tool.id);
                                    setConfirmDelete(null);
                                    await refreshTools();
                                  })}>Delete tool</button>
                                  <button className="ghost" onClick={() => setConfirmDelete(null)}>Keep tool</button>
                                </div>
                              </div>
                            )}
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
                loading={!connectionsLoaded && !error}
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

function scheduleLine(s: Schedule | undefined) {
  if (!s) return null;
  if (s.status === "needs_reapproval") return "Schedule needs your approval again";
  if (s.status === "emergency_paused") return "Schedule paused by emergency stop";
  if (s.status === "paused") return "Schedule paused";
  const when = s.frequency.kind === "daily" ? "Every day" : s.frequency.kind === "weekly" ? "Every week" : "Every month";
  const sends = [s.outputs.email ? `emails ${s.outputs.email.to}` : null, s.outputs.notification ? "notifies you" : null].filter(Boolean);
  const only = s.outputs.sendWhen && s.outputs.sendWhen.kind !== "always" && sends.length ? ` only when ${whenText(s.outputs.sendWhen)}` : "";
  return `${when} at ${s.frequency.time.replace(/^0/, "")}${sends.length ? ` · ${sends.join(" and ")}${only}` : ""}${s.nextRunMs ? ` · ${nextRunText(s.nextRunMs)}` : ""}`;
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

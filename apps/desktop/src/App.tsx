import { useEffect, useState } from 'react';
import type { Backend, ToolSummary } from './backend/types';

const destinations = ['Tools', 'Activity', 'Connections', 'Privacy', 'Settings'] as const;
type Destination = (typeof destinations)[number];

export function App({ backend }: { backend: Backend }) {
  const [destination, setDestination] = useState<Destination>('Tools');
  const [tools, setTools] = useState<ToolSummary[] | null>(null);
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    let active = true;
    setTools(null);
    setFailed(false);
    void backend.listTools().then(
      (result) => { if (active) setTools(result); },
      () => { if (active) setFailed(true); },
    );
    return () => { active = false; };
  }, [backend]);

  return (
    <div className="app-shell">
      <aside className="sidebar">
        <div className="brand">Macroscope</div>
        <nav aria-label="Main navigation">
          {destinations.map((item) => (
            <button
              key={item}
              type="button"
              aria-current={destination === item ? 'page' : undefined}
              onClick={() => setDestination(item)}
            >
              {item}
            </button>
          ))}
        </nav>
        <p className="sidebar-note">Small tools for everyday work.</p>
      </aside>
      <main id="main-content" aria-labelledby="page-title">
        <header>
          <h1 id="page-title">{destination}</h1>
          {destination === 'Tools' && <p>Your saved tools will live here.</p>}
        </header>
        {destination === 'Tools' && (
          failed ? <p role="alert">Tools could not be loaded. Try reopening Macroscope.</p> :
          tools === null ? <p role="status">Loading tools…</p> :
          tools.length === 0 ? (
            <section className="empty-state" aria-labelledby="empty-title">
              <div className="empty-symbol" aria-hidden="true">✳</div>
              <h2 id="empty-title">Make room for the work that matters.</h2>
              <p>Turn a repetitive CSV task into a tool you can check and use again.</p>
              <div className="actions">
                <button type="button" className="primary">Try a sample</button>
                <button type="button">Describe a CSV task</button>
              </div>
            </section>
          ) : (
            <ul className="tools-list">
              {tools.map((tool) => <li key={tool.id}>{tool.name} <span>v{tool.version}</span></li>)}
            </ul>
          )
        )}
        {destination !== 'Tools' && <p className="placeholder">This space is getting ready.</p>}
      </main>
    </div>
  );
}

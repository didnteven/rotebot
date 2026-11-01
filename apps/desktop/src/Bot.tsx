import { safeBotImage, useBotLibrary } from "./BotLibrary";
import { useEffect, useState } from "react";

export type BotState = "idle" | "working" | "building" | "waiting" | "success" | "error" | "paused";
const avatarKey = "macroscope.bot-identities.v1";
function identities(): Record<string, string> {
  try {
    const value: unknown = JSON.parse(localStorage.getItem(avatarKey) || "{}");
    return value && typeof value === "object" && !Array.isArray(value) ? value as Record<string, string> : {};
  } catch { return {}; }
}
export function botIdentity(taskId: string) {
  const seed = identities()[taskId];
  return typeof seed === "string" ? seed : taskId;
}
export function saveBotIdentity(taskId: string, seed: string) {
  try { localStorage.setItem(avatarKey, JSON.stringify({ ...identities(), [taskId]: seed })); window.dispatchEvent(new Event("macroscope-bot-changed")); } catch { /* Stable ID remains the fallback when storage is unavailable. */ }
}
export function botHash(seed: string) {
  let hash = 2166136261;
  for (const character of seed) hash = Math.imul(hash ^ character.charCodeAt(0), 16777619);
  return hash >>> 0;
}

/** Local companion atlas: identity never becomes a URL or executable markup. */
export function BotAvatar({ seed = "macroscope", state = "idle", size = 64 }: {
  seed?: string; state?: BotState; size?: number;
}) {
  const hash = botHash(seed);
  const family = hash % 4;
  const tint = (hash >>> 4) % 6;
  const pose = state === "working" || state === "building" ? 1
    : state === "waiting" ? 2 : state === "success" ? 3
    : state === "error" ? 4 : state === "paused" ? 5 : 0;
  const library = useBotLibrary();
  const customId = seed.startsWith("custom:") ? seed.slice(7) : null;
  const design = customId ? library.designs[customId] : undefined;
  useEffect(() => { if (customId && !design) library.load(customId); }, [customId, design, library.load]);
  const source = safeBotImage(design?.images[pose] ?? design?.images[0]);
  return <span className={`bot-avatar bot-${state}`} style={{ width: size, height: size }} aria-hidden="true">
    {source ? <img className="bot-custom-image" src={source} alt="" draggable={false} /> : <span className={`bot-sprite bot-family-${family} bot-pose-${pose} bot-tint-${tint}`} />}
  </span>;
}

export function BotStatus({ label, seed, state = "working", detail, large = false }: {
  label: string; seed?: string; state?: BotState; detail?: string; large?: boolean;
}) {
  return <div className={`bot-status${large ? " bot-status-large" : ""}`} role="status" aria-live="polite">
    <BotAvatar seed={seed} state={state} size={large ? 128 : 52} />
    <div><strong>{label}</strong>{detail && <p className="muted">{detail}</p>}</div>
  </div>;
}

export function ThemePicker() {
  const [theme, setTheme] = useState(() => {
    try { return localStorage.getItem("macroscope.theme") || "system"; } catch { return "system"; }
  });
  useEffect(() => {
    document.documentElement.dataset.theme = theme;
    try { localStorage.setItem("macroscope.theme", theme); } catch { /* Session preference still works. */ }
  }, [theme]);
  return <label className="theme-picker">Appearance<select aria-label="Appearance" value={theme} onChange={e => setTheme(e.target.value)}>
    <option value="system">System</option><option value="light">Light</option><option value="dark">Dark</option>
  </select></label>;
}

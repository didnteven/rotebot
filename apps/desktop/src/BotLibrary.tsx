import { createContext, useCallback, useContext, useEffect, useRef, useState, type ReactNode } from "react";
import type { Backend, BotDesign } from "./backend/types";

type Library = { designs: Record<string, BotDesign>; load: (id: string) => void; remember: (bot: BotDesign) => void; revision: number };
const Context = createContext<Library>({ designs: {}, load: () => {}, remember: () => {}, revision: 0 });
export function BotLibraryProvider({ backend, children }: { backend: Backend; children: ReactNode }) {
  const [designs, setDesigns] = useState<Record<string, BotDesign>>({});
  const [revision, setRevision] = useState(0);
  const pending = useRef(new Set<string>());
  const epoch = useRef(0);
  const remember = useCallback((bot: BotDesign) => setDesigns(current => ({ ...current, [bot.id]: bot })), []);
  const load = useCallback((id: string) => {
    if (pending.current.has(id)) return;
    pending.current.add(id);
    const requestEpoch = epoch.current;
    void backend.getBotDesign(id).then(bot => { if (requestEpoch === epoch.current) remember(bot); }).catch(() => { /* Built-in bot remains visible if a saved image is missing. */ });
  }, [backend, remember]);
  useEffect(() => {
    const changed = () => setRevision(value => value + 1);
    const deleted = () => { epoch.current++; setDesigns({}); pending.current.clear(); };
    window.addEventListener("macroscope-bot-changed", changed);
    window.addEventListener("macroscope-bots-deleted", deleted);
    return () => { window.removeEventListener("macroscope-bot-changed", changed); window.removeEventListener("macroscope-bots-deleted", deleted); };
  }, []);
  return <Context.Provider value={{ designs, load, remember, revision }}>{children}</Context.Provider>;
}
export function useBotLibrary() { return useContext(Context); }
export function safeBotImage(value: string | undefined): string | undefined {
  return value && value.length <= 12 * 1024 * 1024 && /^data:image\/png;base64,iVBORw0KGgo[A-Za-z0-9+/]*={0,2}$/.test(value) ? value : undefined;
}

import { useEffect, useId, useRef, useState, type KeyboardEvent } from "react";
import { BotAvatar } from "./Bot";

export function BotAppearanceMenu({ seed, disabled, shuffle, customize }: {
  seed: string; disabled: boolean; shuffle: () => void; customize: () => void;
}) {
  const [open, setOpen] = useState(false);
  const id = useId();
  const container = useRef<HTMLDivElement>(null);
  const trigger = useRef<HTMLButtonElement>(null);
  const choices = useRef<(HTMLButtonElement | null)[]>([]);
  useEffect(() => {
    if (!open) return;
    choices.current[0]?.focus();
    const outside = (event: PointerEvent) => {
      if (event.target instanceof Node && !container.current?.contains(event.target)) setOpen(false);
    };
    document.addEventListener("pointerdown", outside);
    return () => document.removeEventListener("pointerdown", outside);
  }, [open]);
  function select(action: () => void) {
    setOpen(false);
    trigger.current?.focus();
    action();
  }
  function navigate(event: KeyboardEvent<HTMLDivElement>) {
    const current = choices.current.indexOf(document.activeElement as HTMLButtonElement);
    if (["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)) {
      event.preventDefault();
      const index = event.key === "Home" ? 0 : event.key === "End" ? 1 : (current + (event.key === "ArrowDown" ? 1 : -1) + 2) % 2;
      choices.current[index]?.focus();
    } else if (event.key === "Escape" || event.key === "Tab") {
      if (event.key === "Escape") { event.preventDefault(); event.stopPropagation(); }
      setOpen(false);
      trigger.current?.focus();
    }
  }
  return <div className="bot-appearance" ref={container}>
    <BotAvatar seed={seed} size={64} />
    <button className="bot-edit-button" ref={trigger} type="button" disabled={disabled} aria-label="Edit character appearance" title="Edit character appearance" aria-haspopup="menu" aria-expanded={open} aria-controls={open ? id : undefined}
      onClick={() => setOpen(value => !value)} onKeyDown={event => { if (event.key === "ArrowDown") { event.preventDefault(); setOpen(true); } }}>
      <span className="bot-edit-glyph" aria-hidden="true"><span className="pencil-icon" /></span>
    </button>
    {open && <div id={id} className="bot-appearance-menu" role="menu" aria-label="Character appearance" onKeyDown={navigate}>
      <button ref={element => { choices.current[0] = element; }} type="button" role="menuitem" onClick={() => select(shuffle)}>Try another character</button>
      <button ref={element => { choices.current[1] = element; }} type="button" role="menuitem" onClick={() => select(customize)}>Create a character</button>
    </div>}
  </div>;
}

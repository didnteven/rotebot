import type { SendWhen } from "./backend/types";

// Mirrors the number reading in crates/app-core/src/conditions.rs, so a choice made here
// finds the same number when the schedule runs.
type Found = { start: number; end: number; value: number };
const digit = (c?: string) => !!c && c >= "0" && c <= "9";
const alpha = (c?: string) => !!c && /[A-Za-z]/.test(c);

function numbers(text: string): Found[] {
  const out: Found[] = [];
  let i = 0;
  while (i < text.length) {
    if (!digit(text[i])) { i++; continue; }
    const start = i;
    let j = i;
    while (j < text.length && (digit(text[j]) || ((text[j] === "," || text[j] === ".") && digit(text[j + 1])))) j++;
    const prev = start > 0 ? text[start - 1] : undefined, beforePrev = start > 1 ? text[start - 2] : undefined, next = text[j];
    const dateOrTime = ("-/:".includes(next ?? "\0") && digit(text[j + 1])) || ("-/:".includes(prev ?? "\0") && digit(beforePrev)) || alpha(prev);
    if (!dateOrTime) {
      const value = Number(text.slice(start, j).replaceAll(",", ""));
      const negative = prev === "-" && !(beforePrev && /[A-Za-z0-9]/.test(beforePrev));
      if (Number.isFinite(value)) out.push({ start: negative ? start - 1 : start, end: j, value: negative ? -value : value });
    }
    i = Math.max(j, i + 1);
  }
  return out;
}

const filler = new Set([" ", ":", "=", "$", "(", " "]);
export function readNumber(output: string, near: string, side: "after" | "before"): number | null {
  near = near.trim();
  if (!near) return numbers(output.split("\n")[0] ?? "")[0]?.value ?? null;
  const all = numbers(output), hay = output.toLowerCase(), needle = near.toLowerCase();
  if (hay.length !== output.length) return null;
  let from = 0;
  for (;;) {
    const at = hay.indexOf(needle, from);
    if (at < 0) return null;
    const begin = at, end = at + needle.length;
    let found: number | undefined;
    if (side === "after") {
      let gap = 0; while (filler.has(output[end + gap] ?? "\0")) gap++;
      if (gap <= 3) found = all.find(n => n.start === end + gap)?.value;
    } else {
      let gap = 0; while (begin - gap > 0 && filler.has(output[begin - gap - 1])) gap++;
      if (gap <= 3) found = all.find(n => n.end === begin - gap)?.value;
    }
    if (found !== undefined) return found;
    from = end;
  }
}

export interface NumberChoice { label: string; near: string; side: "after" | "before"; value: number }

/** The numbers in a stored result, each described the way a person would, e.g. "Tide: 9". */
export function numberChoices(output: string): NumberChoice[] {
  const choices: NumberChoice[] = [];
  const seen = new Set<string>();
  for (const n of numbers(output.slice(0, 4000))) {
    const raw = output.slice(n.start, n.end);
    const words = output.slice(Math.max(0, n.start - 40), n.start).replace(/[\s:=$( ]+$/, "").match(/[A-Za-z][A-Za-z']*$/);
    const unit = output.slice(n.end).match(/^\s?(%|[A-Za-z]{1,3}\b)/);
    const candidates: Array<{ near: string; side: "after" | "before" }> = [];
    if (words) candidates.push({ near: words[0], side: "after" });
    if (unit) candidates.push({ near: unit[1], side: "before" });
    const pick = candidates.find(c => readNumber(output, c.near, c.side) === n.value);
    if (!pick) continue;
    const key = `${pick.side}|${pick.near.toLowerCase()}`;
    if (seen.has(key)) continue;
    seen.add(key);
    choices.push({ ...pick, value: n.value, label: pick.side === "after" ? `${pick.near}: ${raw}` : output.slice(n.start, n.end + (unit?.[0].length ?? 0)) });
    if (choices.length >= 8) break;
  }
  return choices;
}

export function applyChoice(kind: "above" | "below", c: NumberChoice, value = Number.NaN): SendWhen {
  return { kind, near: c.near, side: c.side, value };
}

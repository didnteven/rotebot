import { expect,it } from "vitest";
import { numberChoices,readNumber } from "./conditionChoices";
// Real outputs from live APIs, the same ones the Rust condition tests use.
const TIDE="Mission Bay tide right now (2026-10-31T18:15): -0.98 m, Tide 9% (full being 100%)\n";
const FX="1 EUR = 1.98 NZD\n";
const LOSER="Biggest loser today: LVWR (LiveWire Group Inc. Common Stock) -43.744% at $0.9001\n";
const QUAKE="Latest NZ quake: magnitude 3.2, 20 km west of Porangahau (2026-10-30T06:00:40.350Z)\n";
it("offers the numbers a person would name, never dates or times",()=>{
  const labels=numberChoices(TIDE).map(c=>c.label);
  expect(labels).toEqual(["-0.98 m","Tide: 9","being: 100"]);
  expect(labels.join()).not.toMatch(/2026|18:15/);
  expect(numberChoices(QUAKE).map(c=>c.label)).toEqual(expect.arrayContaining(["magnitude: 3.2","20 km"]));
  expect(numberChoices(LOSER).map(c=>c.label)).toContain("-43.744%");
});
it("every choice finds the number it was offered for, as the schedule will",()=>{
  for(const out of [TIDE,FX,LOSER,QUAKE]) for(const c of numberChoices(out)) expect(readNumber(out,c.near,c.side)).toBe(c.value);
  expect(readNumber(TIDE,"Tide","after")).toBe(9);
  expect(readNumber(FX,"EUR","before")).toBe(1);
  expect(numberChoices("nothing numeric here")).toEqual([]);
});

// How each isoline family looks (PLAN.md M21.2; the dataviz skill): a colour that follows the family across diagrams,
// a dash pattern that goes with the colour (the secondary encoding the third colour's 6-8 CVD band needs), and a name.
// Families that share a diagram never share a colour (diagrams.ts; tests/unit/diagram.test.ts).
import { formatNumber } from "../lib/format";
import { output, type Output } from "../lib/outputs";
import { displayUnit, fromSI, type System, type Unit } from "../lib/units";
import type { Key } from "../sample/diagrams";

export interface FamilyStyle {
  name: string;
  /** A colour token of app.css. */
  colour: string;
  dash: readonly number[];
  width: number;
}

const SLOTS = [
  { colour: "--iso-1", dash: [] },
  { colour: "--iso-2", dash: [6, 4] },
  { colour: "--iso-3", dash: [1.5, 3] },
] as const;

const FAMILIES: Record<Exclude<Key, "Q">, { name: string; slot: 0 | 1 | 2 }> = {
  T: { name: "Isotherms", slot: 0 },
  Hmass: { name: "Isenthalps", slot: 0 },
  P: { name: "Isobars", slot: 1 },
  Smass: { name: "Isentropes", slot: 1 },
  Dmass: { name: "Isochores", slot: 2 },
};

export function styleOf(held: Key): FamilyStyle {
  if (held === "Q") return { name: "Lines of constant quality", colour: "--fg-muted", dash: [2, 3], width: 1 };
  const { name, slot } = FAMILIES[held];
  return { name, ...SLOTS[slot], width: 1.5 };
}

/** The unit a state variable shows in. */
export function unitOfKey(key: Key, system: System): Unit {
  return displayUnit((output(key) as Output).kind, system);
}

/** A line's value in the shown units, the way its label reads. */
export function lineLabel(held: Key, value: number, system: System): string {
  if (held === "Q") return `q = ${value}`;
  const unit = unitOfKey(held, system);
  return `${formatNumber(fromSI(value, unit))} ${unit.label}`;
}

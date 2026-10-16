// The fluids the calculator offers: the five the browser package embeds (`fluids-core`, E7), under their canonical
// names. M21.4 replaces this list with the registry's own, all 136 from packs.

export interface FluidChoice {
  name: string;
  label: string;
}

export const FLUIDS: readonly FluidChoice[] = [
  { name: "Water", label: "Water" },
  { name: "CarbonDioxide", label: "Carbon dioxide" },
  { name: "Nitrogen", label: "Nitrogen" },
  { name: "n-Propane", label: "Propane" },
  { name: "R134a", label: "R134a" },
];

/** A ready-made state: a fluid and two inputs, in SI. */
export interface Preset {
  label: string;
  fluid: string;
  a: [string, number];
  b: [string, number];
}

export const PRESETS: readonly Preset[] = [
  { label: "Water boiling at 1 atm", fluid: "Water", a: ["P", 101325], b: ["Q", 0] },
  { label: "R134a saturated vapour at 0 °C", fluid: "R134a", a: ["T", 273.15], b: ["Q", 1] },
  { label: "CO₂ above its critical point", fluid: "CarbonDioxide", a: ["T", 310], b: ["P", 8e6] },
  { label: "Liquid nitrogen at 77 K", fluid: "Nitrogen", a: ["T", 77], b: ["P", 101325] },
  { label: "Propane gas at 300 K, 1 bar", fluid: "n-Propane", a: ["T", 300], b: ["P", 1e5] },
];

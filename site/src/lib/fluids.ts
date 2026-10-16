// The calculator's ready-made states. Its fluids are the registry's and packs/index.json's (PLAN.md M21.4): the five
// the browser package embeds (`fluids-core`, E7) and every other from its pack, fetched on first use.

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
  { label: "R32, from its pack, boiling at 1 atm", fluid: "R32", a: ["P", 101325], b: ["Q", 0] },
];

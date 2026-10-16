// The units the page shows and takes. The package speaks SI only; the page converts at its edge, by the SI brochure's
// definitions (BIPM, 9th edition, 2019): 0 °C = 273.15 K, 1 bar = 10⁵ Pa, the prefixes µ, m, k, M = 10⁻⁶ … 10⁶.

/** A unit: SI value = value · scale + offset. */
export interface Unit {
  label: string;
  scale: number;
  offset: number;
}

export type KindId =
  | "temperature"
  | "pressure"
  | "massDensity"
  | "molarDensity"
  | "specificEnergy"
  | "molarEnergy"
  | "specificEntropy"
  | "molarEntropy"
  | "molarMass"
  | "speed"
  | "viscosity"
  | "conductivity"
  | "surfaceTension"
  | "inversePressure"
  | "inverseTemperature"
  | "molarVolume"
  | "molarVolumeSquared"
  | "molarVolumePerKelvin"
  | "molarVolumeSquaredPerKelvin"
  | "dipoleMoment"
  | "moleFraction"
  | "massFraction"
  | "dimensionless";

/** A quantity's units, its SI unit first. */
export interface Kind {
  id: KindId;
  units: readonly [Unit, ...Unit[]];
}

const u = (label: string, scale = 1, offset = 0): Unit => ({ label, scale, offset });

export const KINDS: readonly Kind[] = [
  { id: "temperature", units: [u("K"), u("°C", 1, 273.15)] },
  { id: "pressure", units: [u("Pa"), u("kPa", 1e3), u("bar", 1e5), u("MPa", 1e6)] },
  { id: "massDensity", units: [u("kg/m³")] },
  { id: "molarDensity", units: [u("mol/m³"), u("mol/dm³", 1e3)] },
  { id: "specificEnergy", units: [u("J/kg"), u("kJ/kg", 1e3)] },
  { id: "molarEnergy", units: [u("J/mol"), u("kJ/mol", 1e3)] },
  { id: "specificEntropy", units: [u("J/(kg·K)"), u("kJ/(kg·K)", 1e3)] },
  { id: "molarEntropy", units: [u("J/(mol·K)")] },
  { id: "molarMass", units: [u("kg/mol"), u("g/mol", 1e-3)] },
  { id: "speed", units: [u("m/s")] },
  { id: "viscosity", units: [u("Pa·s"), u("mPa·s", 1e-3), u("µPa·s", 1e-6)] },
  { id: "conductivity", units: [u("W/(m·K)"), u("mW/(m·K)", 1e-3)] },
  { id: "surfaceTension", units: [u("N/m"), u("mN/m", 1e-3)] },
  { id: "inversePressure", units: [u("1/Pa"), u("1/MPa", 1e-6)] },
  { id: "inverseTemperature", units: [u("1/K")] },
  { id: "molarVolume", units: [u("m³/mol")] },
  { id: "molarVolumeSquared", units: [u("m⁶/mol²")] },
  { id: "molarVolumePerKelvin", units: [u("m³/(mol·K)")] },
  { id: "molarVolumeSquaredPerKelvin", units: [u("m⁶/(mol²·K)")] },
  { id: "dipoleMoment", units: [u("C·m")] },
  { id: "moleFraction", units: [u("mol/mol")] },
  { id: "massFraction", units: [u("kg/kg")] },
  { id: "dimensionless", units: [u("")] },
];

const KIND = Object.fromEntries(KINDS.map((k) => [k.id, k])) as Record<KindId, Kind>;

/** The unit of `kind` labelled `label`; a programming error if there is none. */
export function unit(kind: KindId, label: string): Unit {
  const found = KIND[kind].units.find((x) => x.label === label);
  if (!found) throw new Error(`no unit ${label} of ${kind}`);
  return found;
}

export function toSI(value: number, unit: Unit): number {
  return value * unit.scale + unit.offset;
}

export function fromSI(value: number, unit: Unit): number {
  return (value - unit.offset) / unit.scale;
}

/** SI, or the engineer's units: °C, bar, kJ and their kin. */
export type System = "si" | "engineering";

export const SYSTEMS: readonly { id: System; label: string }[] = [
  { id: "si", label: "SI" },
  { id: "engineering", label: "°C, bar, kJ" },
];

const ENGINEERING: Partial<Record<KindId, string>> = {
  temperature: "°C",
  pressure: "bar",
  molarDensity: "mol/dm³",
  specificEnergy: "kJ/kg",
  molarEnergy: "kJ/mol",
  specificEntropy: "kJ/(kg·K)",
  molarMass: "g/mol",
  viscosity: "µPa·s",
  conductivity: "mW/(m·K)",
  surfaceTension: "mN/m",
  inversePressure: "1/MPa",
};

/** The unit a quantity of `kind` shows in, in `system`: SI, or the engineer's where the kind has one. */
export function displayUnit(kind: KindId, system: System): Unit {
  const units = KIND[kind].units;
  if (system === "si") return units[0];
  return units.find((x) => x.label === ENGINEERING[kind]) ?? units[0];
}

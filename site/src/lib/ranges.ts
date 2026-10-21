// A fluid's plotted ranges and the grids over them (PLAN.md M21.2), from its own limits (the package's Props1SI): the
// sweeps' and the benchmark's. The diagrams take theirs from the package (phasekit-diagram, M21.6).

/** The constants a diagram needs, in this order. */
export const LIMIT_KEYS = [
  "T_critical",
  "p_critical",
  "rhomass_critical",
  "T_triple",
  "p_triple",
  "T_min",
  "T_max",
  "P_max",
] as const;

export interface Limits {
  Tc: number;
  pc: number;
  rhoc: number;
  Ttriple: number;
  ptriple: number;
  Tmin: number;
  Tmax: number;
  pmax: number;
}

/** The limits from the constants in LIMIT_KEYS' order, or null where any is missing. */
export function limits(values: readonly (number | null)[]): Limits | null {
  if (values.length !== LIMIT_KEYS.length || !values.every((v) => Number.isFinite(v))) return null;
  const [Tc, pc, rhoc, Ttriple, ptriple, Tmin, Tmax, pmax] = values as number[];
  return { Tc, pc, rhoc, Ttriple, ptriple, Tmin, Tmax, pmax } as Limits;
}

export interface Ranges {
  T: [number, number];
  P: [number, number];
}

/** From the triple point (or the equation of state's lowest T) to 1.6 T_c, and from p_c / 10⁴ (or the triple point's
 *  pressure) to 10 p_c, within the equation of state's limits. */
export function ranges(l: Limits): Ranges {
  return {
    T: [Math.max(l.Tmin, l.Ttriple), Math.min(l.Tmax, l.Tc * 1.6)],
    P: [Math.max(l.ptriple, l.pc / 1e4), Math.min(l.pmax, l.pc * 10)],
  };
}

/** n values from a to b, evenly spaced, ending exactly on b. */
export function linGrid(a: number, b: number, n: number): number[] {
  return Array.from({ length: n }, (_, i) => (i === n - 1 ? b : a + ((b - a) * i) / (n - 1)));
}

/** n values from a to b, evenly spaced in their logarithm, ending exactly on b. */
export function logGrid(a: number, b: number, n: number): number[] {
  return Array.from({ length: n }, (_, i) => (i === n - 1 ? b : a * (b / a) ** (i / (n - 1))));
}

/** n temperatures from exactly `low` up to just below T_c, closing in on it, where the dome narrows fastest. */
export function domeTemperatures(low: number, tc: number, n: number): number[] {
  return Array.from({ length: n }, (_, i) => (i === 0 ? low : tc - (tc - low) * (1 - i / n) ** 2));
}

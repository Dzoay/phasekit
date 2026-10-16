// A diagram's ranges, grids and isoline values (PLAN.md M21.2), from the fluid's own limits (the package's Props1SI).
import { scaleLinear } from "d3-scale";
import type { Key } from "./diagrams";

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

/** n temperatures from `low` up to just below T_c, closing in on it, where the dome narrows fastest. */
export function domeTemperatures(low: number, tc: number, n: number): number[] {
  return Array.from({ length: n }, (_, i) => tc - (tc - low) * (1 - i / n) ** 2);
}

/** The values a family's lines take, strictly inside `range`: 1-2-5 per decade for the logarithmic quantities while
 *  that is at most 12 lines, decades past that; nice linear ticks for the rest (tenths, for quality's [0, 1]). */
export function values(key: Key, range: readonly [number, number]): number[] {
  const [a, b] = range;
  if (key === "P" || key === "Dmass") {
    const decades = (mantissas: number[]) =>
      Array.from({ length: Math.ceil(Math.log10(b)) - Math.floor(Math.log10(a)) + 1 }, (_, i) =>
        mantissas.map((m) => m * 10 ** (Math.floor(Math.log10(a)) + i)),
      )
        .flat()
        .filter((v) => a < v && v < b);
    const fine = decades([1, 2, 5]);
    return fine.length <= 12 ? fine : decades([1]);
  }
  return scaleLinear()
    .domain([a, b])
    .ticks(8)
    .filter((v) => a < v && v < b);
}

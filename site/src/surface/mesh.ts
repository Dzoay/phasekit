// A fluid's surface as the page draws it in WebGL (PLAN.md M26.2; user decisions DG7, DG8): the package's mesh
// (phasekit-diagram's, through `Fluid.surface`) placed in a cube, each axis's span scaled to [−1, 1], a logarithmic one
// in its logarithm. p–v–T stands p up over v and T, p and v logarithmic; Gibbs's u–s–v stands u up over s and v, v
// logarithmic as it spans as many decades. It adds no numerics: every vertex is the package's state.
import type { SurfaceValue } from "../engine/protocol";

export type SurfaceKind = "pvt" | "usv";
export type AxisKey = "v" | "p" | "T" | "u" | "s";

export interface Axis {
  key: AxisKey;
  log: boolean;
  /** The least and greatest value over the vertices, in SI. */
  span: readonly [number, number];
}

/** Each surface's axes: across, up and deep, as three.js's x, y and z. */
export const KINDS: Readonly<Record<SurfaceKind, { name: string; axes: readonly [AxisKey, AxisKey, AxisKey] }>> = {
  pvt: { name: "p–v–T", axes: ["v", "p", "T"] },
  usv: { name: "u–s–v", axes: ["s", "u", "v"] },
};

const LOG: ReadonlySet<AxisKey> = new Set(["v", "p"]);

/** The kind a URL names; p–v–T for anything else. */
export function kindOf(raw: string | null): SurfaceKind {
  return raw === "usv" ? "usv" : "pvt";
}

/** Each vertex's value of a quantity, v = 1/ρ. */
export function valuesOf(m: SurfaceValue, key: AxisKey): Float64Array {
  return key === "v" ? m.rho.map((rho) => 1 / rho) : m[key];
}

/** The least and greatest finite value; [NaN, NaN] where there is none. */
export function span(values: Float64Array): [number, number] {
  let [lo, hi] = [Infinity, -Infinity];
  for (const v of values) {
    if (Number.isFinite(v)) [lo, hi] = [Math.min(lo, v), Math.max(hi, v)];
  }
  return lo <= hi ? [lo, hi] : [NaN, NaN];
}

/** A value's place on its axis, from −1 at the span's least to 1 at its greatest; 0 where the span is a point. */
export function place(value: number, axis: Axis): number {
  const at = (x: number) => (axis.log ? Math.log10(x) : x);
  const [lo, hi] = [at(axis.span[0]), at(axis.span[1])];
  return hi > lo ? (2 * (at(value) - lo)) / (hi - lo) - 1 : 0;
}

/** The token of a phase's colour on a surface: `--surface-<phase>`, its underscores as hyphens. */
export function surfaceToken(phase: string): string {
  return `--surface-${phase.replaceAll("_", "-")}`;
}

/** The surface's axes, and each vertex's place in the cube, three a vertex: across and up as three.js's x and y, deep
 *  as its −z, away from a viewer who looks down −z. */
export function positions(m: SurfaceValue, kind: SurfaceKind): { axes: Axis[]; xyz: Float32Array } {
  const columns = KINDS[kind].axes.map((key) => valuesOf(m, key));
  const axes = KINDS[kind].axes.map((key, k) => ({ key, log: LOG.has(key), span: span(columns[k] as Float64Array) }));
  const xyz = new Float32Array(3 * m.T.length);
  const sign = [1, 1, -1];
  columns.forEach((column, k) =>
    column.forEach((value, i) => (xyz[3 * i + k] = (sign[k] as number) * place(value, axes[k] as Axis))),
  );
  return { axes, xyz };
}

/** The dome as one line: up the saturated liquid, through the critical point where there is one, down the vapour. */
export function domePath(m: SurfaceValue): number[] {
  return [...m.liquid, ...m.critical, ...[...m.vapour].reverse()];
}

/** Each vertex's colour, three channels a vertex, from its phase's. */
export function vertexColours(
  m: SurfaceValue,
  rgb: (phase: string) => readonly [number, number, number],
): Float32Array {
  const byCode = m.phaseNames.map(rgb);
  const out = new Float32Array(3 * m.phases.length);
  m.phases.forEach((code, i) => out.set(byCode[code] ?? [0, 0, 0], 3 * i));
  return out;
}

/** The phases the surface has, in the legend's order of `order`. */
export function phasesPresent(m: SurfaceValue, order: readonly string[]): string[] {
  const present = new Set([...m.phases].map((code) => m.phaseNames[code]));
  return order.filter((phase) => present.has(phase));
}

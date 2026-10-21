// A fluid's surface as the page draws it in WebGL (PLAN.md M26.2; user decisions DG7, DG8): the package's mesh
// (phasekit-diagram's, through `Fluid.surface`) placed in a cube, each axis's span scaled to [−1, 1], a logarithmic one
// in its logarithm. p–v–T stands p up over v and T, p and v logarithmic; Gibbs's u–s–v stands u up over s and v, v
// logarithmic as it spans as many decades. It adds no numerics: every vertex is the package's state.
import { scaleLinear } from "d3-scale";
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

/** Whether a vertex is on the dome: a saturated state, which borders the two-phase region and a one-phase one. */
function onDome(m: SurfaceValue, i: number): boolean {
  const q = m.Q[i];
  return q === 0 || q === 1;
}

/** Each triangle's phase code: the one most of its vertices off the dome have, the first of them on a tie; where all
 *  three are on the dome, theirs (the two-phase region's). One colour a triangle, so that no phase bleeds into the
 *  next. */
export function trianglePhases(m: SurfaceValue): Uint8Array {
  const out = new Uint8Array(m.triangles.length / 3);
  for (let t = 0; t < out.length; t++) {
    const corners = [...m.triangles.subarray(3 * t, 3 * t + 3)];
    const off = corners.filter((i) => !onDome(m, i));
    const codes = (off.length > 0 ? off : corners).map((i) => m.phases[i] as number);
    const count = (code: number) => codes.filter((c) => c === code).length;
    out[t] = codes.reduce((best, code) => (count(code) > count(best) ? code : best));
  }
  return out;
}

/** Each triangle's colour, three channels a triangle, from its phase's; black for a code with no name. */
export function triangleColours(
  m: SurfaceValue,
  rgb: (phase: string) => readonly [number, number, number],
): Float32Array {
  const byCode = m.phaseNames.map(rgb);
  const phases = trianglePhases(m);
  const out = new Float32Array(3 * phases.length);
  phases.forEach((code, t) => {
    const rgb = byCode[code];
    if (rgb) out.set(rgb, 3 * t);
  });
  return out;
}

/** An axis's labelled values inside its span: on a logarithmic axis its decades, every other one past eight; on a
 *  linear one about five round numbers. */
export function ticks(axis: Axis): number[] {
  const [lo, hi] = axis.span;
  if (!(lo < hi)) return [];
  if (!axis.log) return scaleLinear().domain([lo, hi]).ticks(5);
  const [first, last] = [Math.ceil(Math.log10(lo)), Math.floor(Math.log10(hi))];
  const every = last - first + 1 > 8 ? 2 : 1;
  const decades: number[] = [];
  for (let k = first; k <= last; k += every) decades.push(Number(`1e${k}`));
  return decades;
}

const SUPERSCRIPT = "⁰¹²³⁴⁵⁶⁷⁸⁹";

/** A tick's label: on a logarithmic axis its decade as 10ᵏ, on a linear one the value as `format` writes it. */
export function tickLabel(value: number, axis: Axis, format: (v: number) => string): string {
  if (!axis.log) return format(value);
  const k = Math.round(Math.log10(value));
  const digits = [...String(Math.abs(k))].map((d) => SUPERSCRIPT[Number(d)]).join("");
  return `10${k < 0 ? "⁻" : ""}${digits}`;
}

/** The state's (T, ρ) at a point of a triangle, by its barycentric weights: T linear and ρ in its logarithm between the
 *  corners, as the mesh lies over T and ln ρ. */
export function stateAt(
  m: SurfaceValue,
  triangle: number,
  weights: readonly [number, number, number],
): [number, number] {
  const corners = [...m.triangles.subarray(3 * triangle, 3 * triangle + 3)];
  let [t, lnRho] = [0, 0];
  corners.forEach((i, k) => {
    const w = weights[k] as number;
    t += w * (m.T[i] as number);
    lnRho += w * Math.log(m.rho[i] as number);
  });
  return [t, Math.exp(lnRho)];
}

/** The phases the surface has, in the legend's order of `order`. */
export function phasesPresent(m: SurfaceValue, order: readonly string[]): string[] {
  const present = new Set([...m.phases].map((code) => m.phaseNames[code]));
  return order.filter((phase) => present.has(phase));
}

// Sweeps (PLAN.md M21.3): an output against T along isobars, against p along isotherms, or against T along saturation
// for each phase. Every point is the package's batch value at the inputs it carries (`Exact`); along saturation the
// latent heat is the package's h at q = 1 less its h at q = 0, at the same T. A failed cell is a gap.
import type { BatchValue } from "../engine/protocol";
import { domeTemperatures, linGrid, logGrid, ranges, type Limits } from "../lib/ranges";

/** The package's batch over one fluid: the page's worker in the browser, the handler itself in tests. */
export type Batch = (
  outputs: string[],
  name1: string,
  values1: Float64Array,
  name2: string,
  values2: Float64Array,
) => Promise<BatchValue>;

/** A point's answer: its outputs' cells, which of them are values, and its inputs, `[name1, value1, name2, value2]`. */
export interface Row {
  cells: number[];
  ok: boolean[];
  inputs: readonly [string, number, string, number];
}

/** The highest cell code that is a value: 0 success, 1 a flagged extrapolation (PLAN.md M10.8). */
const VALUE = 1;

/** The batch's answer as rows, one a point. */
export async function rows(
  batch: Batch,
  outputs: string[],
  name1: string,
  values1: readonly number[],
  name2: string,
  values2: readonly number[],
): Promise<Row[]> {
  const answer = await batch(outputs, name1, Float64Array.from(values1), name2, Float64Array.from(values2));
  return values1.map((v1, i) => {
    const at = i * outputs.length;
    return {
      cells: outputs.map((_, k) => answer.values[at + k] as number),
      ok: outputs.map((_, k) => (answer.status[at + k] as number) <= VALUE),
      inputs: [name1, v1, name2, values2[i] as number],
    };
  });
}

export type SweepMode = "isobars" | "isotherms" | "saturation";

export const MODES: readonly SweepMode[] = ["isobars", "isotherms", "saturation"];

/** The latent heat, h″ − h′: an output of a sweep along saturation only. */
export const LATENT = "latent_heat";

/** The most lines a sweep draws. */
export const MAX_LINES = 6;

export interface SweepPoint {
  x: number;
  y: number;
  inputs: Row["inputs"];
}

export interface SweepLine {
  /** What the line holds: an isobar's p, an isotherm's T, a saturated phase's q; null for the latent heat. */
  held: "P" | "T" | "Q" | null;
  value: number;
  /** In the sweep's order, null where a cell failed. */
  points: (SweepPoint | null)[];
}

export interface Sweep {
  /** The x axis: T, or p on a logarithmic axis. */
  x: "T" | "P";
  lines: SweepLine[];
}

const repeat = (v: number, n: number): number[] => Array.from({ length: n }, () => v);

/** The lines a sweep holds by default: isobars about p_c, isotherms about T_c, each to two significant figures. */
export function defaultValues(mode: SweepMode, l: Limits): number[] {
  if (mode === "isobars") return [0.5, 1.1, 1.5, 3].map((f) => Number((l.pc * f).toPrecision(2)));
  if (mode === "isotherms") return [0.9, 1.05, 1.2, 1.5].map((f) => Number((l.Tc * f).toPrecision(2)));
  return [];
}

export async function sampleSweep(
  mode: SweepMode,
  output: string,
  values: readonly number[],
  l: Limits,
  batch: Batch,
  n = 200,
): Promise<Sweep> {
  const r = ranges(l);
  const point = (x: number, row: Row): SweepPoint | null =>
    row.ok[0] ? { x, y: row.cells[0] as number, inputs: row.inputs } : null;
  if (mode === "saturation") {
    const ts = domeTemperatures(r.T[0], l.Tc, n);
    const phase = (q: number, key: string) => rows(batch, [key], "T", ts, "Q", repeat(q, ts.length));
    if (output === LATENT) {
      const [liquid, vapour] = [await phase(0, "Hmass"), await phase(1, "Hmass")];
      const points = vapour.map((v, i) => {
        const h = liquid[i] as Row;
        return v.ok[0] && h.ok[0]
          ? { x: ts[i] as number, y: (v.cells[0] as number) - (h.cells[0] as number), inputs: v.inputs }
          : null;
      });
      return { x: "T", lines: [{ held: null, value: 0, points }] };
    }
    const lines: SweepLine[] = [];
    for (const q of [0, 1]) {
      const found = await phase(q, output);
      lines.push({ held: "Q", value: q, points: found.map((row, i) => point(ts[i] as number, row)) });
    }
    return { x: "T", lines };
  }
  const isobars = mode === "isobars";
  const xs = isobars ? linGrid(r.T[0], r.T[1], n) : logGrid(r.P[0], r.P[1], n);
  const lines: SweepLine[] = [];
  for (const value of values.slice(0, MAX_LINES)) {
    const held = repeat(value, xs.length);
    const found = isobars
      ? await rows(batch, [output], "T", xs, "P", held)
      : await rows(batch, [output], "T", held, "P", xs);
    lines.push({ held: isobars ? "P" : "T", value, points: found.map((row, i) => point(xs[i] as number, row)) });
  }
  return { x: isobars ? "T" : "P", lines };
}

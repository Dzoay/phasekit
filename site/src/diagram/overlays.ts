// What the diagrams draw over a fluid's planes beyond its lines (PLAN.md M26.6): its characteristic curves, on p–T and
// T–ρ, the parts inside the plot (user decision DG10), and an output's map, its bands' polygons filled under its
// contour lines (DG11). The package computes both (phasekit-diagram's M26.3 and M26.4, through `Fluid.curves` and
// `Fluid.map`); this reads its typed arrays and adds no numerics.
import type { CurveRunsValue, CurvesValue, MapValue, RunsValue } from "../engine/protocol";
import { vertices, type Vertex } from "./data";
import type { DiagramId } from "./planes";

/** A state on a curve or an isotherm. */
export interface CurveState {
  T: number;
  rho: number;
  p: number;
  Z: number;
}

/** A characteristic curve: its name, the package's, its lines and where it meets zero density, if it does. */
export interface Curve {
  name: string;
  lines: CurveState[][];
  zeroDensity: number | null;
}

/** An isotherm of the compressibility chart: its runs, Z against p, split at the saturation pressure. */
export interface ChartIsotherm {
  T: number;
  runs: CurveState[][];
}

export interface CurvesData {
  curves: Curve[];
  chart: ChartIsotherm[];
}

/** How each curve looks and reads, by the package's name, in Brown's order. */
export const CURVES: Readonly<Record<string, { name: string; colour: string; dash: readonly number[] }>> = {
  Ideal: { name: "Ideal curve (Z = 1)", colour: "--curve-1", dash: [] },
  Boyle: { name: "Boyle curve", colour: "--curve-2", dash: [7, 3] },
  JouleThomsonInversion: { name: "Joule–Thomson inversion curve", colour: "--curve-3", dash: [2, 3] },
  JouleInversion: { name: "Joule inversion curve", colour: "--curve-4", dash: [10, 3, 2, 3] },
};

/** The planes the curves are drawn on, and what each draws across and up. */
export const CURVE_PLANES: Readonly<Partial<Record<DiagramId, readonly [keyof CurveState, keyof CurveState]>>> = {
  pt: ["T", "p"],
  trho: ["rho", "T"],
};

/** Each run's states; a run of the chart is cut between two `QT` states, the saturated vapour and liquid, which share
 *  the saturation pressure and between which nothing is drawn. */
function runsOf(r: CurveRunsValue, pairKeys: readonly string[], cut: boolean): CurveState[][] {
  const quality = (i: number) => pairKeys[2 * (r.pairs[i] as number)] === "Q";
  const out: CurveState[][] = [];
  r.starts.subarray(1).forEach((end, k) => {
    const start = r.starts[k] as number;
    let run: CurveState[] = [];
    for (let i = start; i < end; i++) {
      // A cut at a run's first vertex, after another run's saturated state, leaves an empty run, which the chart drops.
      if (cut && quality(i) && quality(i - 1)) {
        out.push(run);
        run = [];
      }
      run.push({ T: r.T[i] as number, rho: r.rho[i] as number, p: r.p[i] as number, Z: r.Z[i] as number });
    }
    out.push(run);
  });
  return out;
}

/** The package's curves and chart as the page draws them. */
export function readCurves(c: CurvesValue): CurvesData {
  const lines = runsOf(c.lines, c.pairKeys, false);
  const curves = c.names.map((name, i) => {
    const zero = c.zeroDensity[i] as number;
    const own = lines.slice(c.curves[i], c.curves[i + 1]);
    return { name, lines: own, zeroDensity: Number.isNaN(zero) ? null : zero };
  });
  // Each isotherm is one run of the package's, cut at its saturation pressure; the chart keeps them by isotherm.
  const chart = Array.from(c.isotherms, (T, j) => {
    const one = { ...c.chart, starts: c.chart.starts.slice(j, j + 2) };
    return { T, runs: runsOf(one, c.pairKeys, true).filter((r) => r.length > 0) };
  });
  return { curves, chart };
}

/** A band of a map: the states from `low` up to `high` (null where unbounded), as polygons. */
export interface Band {
  low: number | null;
  high: number | null;
  polygons: Vertex[][];
}

export interface MapData {
  key: string;
  /** The output's least and greatest over the map's grid. */
  values: [number, number];
  levels: number[];
  /** Each level's contour lines. */
  contours: Vertex[][][];
  bands: Band[];
}

/** Each run's vertices. */
function runVertices(r: RunsValue, pairKeys: readonly string[]): Vertex[][] {
  const all = vertices(r.vertices, pairKeys);
  return Array.from(r.starts.subarray(1), (end, k) => all.slice(r.starts[k], end));
}

const bound = (v: number) => (Number.isNaN(v) ? null : v);

/** The package's map as the page draws it. */
export function readMap(m: MapValue): MapData {
  const lines = runVertices(m.lines, m.pairKeys);
  const polygons = runVertices(m.polygons, m.pairKeys);
  return {
    key: m.key,
    values: [m.values[0] as number, m.values[1] as number],
    levels: Array.from(m.levels),
    contours: Array.from(m.levels, (_, i) => lines.slice(m.contours[i], m.contours[i + 1])),
    bands: Array.from(m.low, (low, j) => ({
      low: bound(low),
      high: bound(m.high[j] as number),
      polygons: polygons.slice(m.bands[j], m.bands[j + 1]),
    })),
  };
}

/** The outputs a map offers, by key: the planes' own quantities and the ones a map shows best. */
export const MAP_OUTPUTS = [
  "T",
  "P",
  "Dmass",
  "Hmass",
  "Smass",
  "Umass",
  "Cpmass",
  "Cvmass",
  "speed_of_sound",
  "Z",
  "viscosity",
  "conductivity",
] as const;

/** A colour as its red, green and blue, from `rgb(r, g, b)` or `#rrggbb`; null for anything else. */
export function channels(colour: string): [number, number, number] | null {
  const rgb = /^rgba?\(\s*(\d+)[\s,]+(\d+)[\s,]+(\d+)/.exec(colour);
  if (rgb) return [Number(rgb[1]), Number(rgb[2]), Number(rgb[3])];
  const hex = /^#([0-9a-f]{2})([0-9a-f]{2})([0-9a-f]{2})$/i.exec(colour);
  return hex ? [parseInt(hex[1] as string, 16), parseInt(hex[2] as string, 16), parseInt(hex[3] as string, 16)] : null;
}

/** Band j of n's fill: evenly from `low` to `high`, a sequential ramp; `low` alone where either is no colour. */
export function bandColour(j: number, n: number, low: string, high: string): string {
  const [a, b] = [channels(low), channels(high)];
  if (!a || !b) return low;
  const f = n > 1 ? j / (n - 1) : 0;
  const mix = a.map((c, i) => Math.round(c + ((b[i] as number) - c) * f));
  return `rgb(${mix.join(", ")})`;
}

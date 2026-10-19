// The phase regions (PLAN.md M21.9): each a polygon whose edges are the package's lines, sampled once per fluid with
// every diagram's axes among the outputs, so a diagram, a size or a zoom costs no call. Each region's phase is the
// package's at a state inside it; the page names no phase itself. A failed cell is left out of an edge, so where the
// package has no state an edge cuts across to the next state it has; on the cold side, where the package's melting
// line ends the liquid, the edge follows it.
//
// In (T, p) the regions are the four quadrants about the critical point, the low-T, low-p one split by the
// saturation curve into liquid, liquid + vapour and gas. T runs from the plotted lowest to T_max and p from below the
// saturation pressure at that lowest T (so every fluid's regions have one shape) to P_max.
import type { Limits } from "./ranges";
import { awayFrom, domeTemperatures, linGrid, logGrid, ranges } from "./ranges";
import { rows, type Batch, type Row } from "./lines";

/** The keys every edge vertex carries: each diagram's axes. */
export const AXES = ["T", "P", "Hmass", "Smass", "Dmass"] as const;
export type AxisKey = (typeof AXES)[number];

/** A state on an edge, by its value of each axis key. */
export type Corner = Record<AxisKey, number>;

export interface Region {
  /** The package's name of the phase (`State.phase`). */
  phase: string;
  outline: Corner[];
}

export interface Regions {
  regions: Region[];
  /** The critical isotherm and isobar, each through the critical point, broken where a cell failed. */
  isotherm: Corner[][];
  isobar: Corner[][];
  /** The critical point, the package's state at (T_c, ρ_c), where it has one: an edge without it joins across. */
  critical: Corner | null;
  /** The fluid's critical temperature and pressure, the package's constants. */
  Tc: number;
  pc: number;
}

/** The package's phase at the state of an input pair; null where it refuses the state. */
export type PhaseAt = (name1: string, value1: number, name2: string, value2: number) => Promise<string | null>;

const corner = (r: Row): Corner | null =>
  r.ok.every(Boolean) ? (Object.fromEntries(AXES.map((k, i) => [k, r.cells[i]])) as Corner) : null;

const repeat = (v: number, n: number): number[] => Array.from({ length: n }, () => v);

/** Runs of consecutive states, broken at each failed cell; a run of one draws nothing and is dropped. */
export function runs(states: readonly (Corner | null)[]): Corner[][] {
  const out: Corner[][] = [[]];
  for (const s of states) {
    if (s) (out.at(-1) as Corner[]).push(s);
    else out.push([]);
  }
  return out.filter((r) => r.length > 1);
}

/** How many isobars the cold edge crosses, on either side of p_c, and how many halvings find each its temperature. */
export const COLD_ROWS = 48;
const HALVINGS = 12;

export async function sampleRegions(l: Limits, batch: Batch, phaseAt: PhaseAt, n = 160): Promise<Regions> {
  const { Tc, pc } = l;
  const [low, high] = [ranges(l).T[0], l.Tmax];
  const keys = [...AXES];
  const states = async (name1: string, values1: number[], name2: string, values2: number[]) =>
    (await rows(batch, keys, name1, values1, name2, values2)).map(corner);
  // An isotherm over pressures, an isobar over temperatures, each from `a` to `b`.
  const isotherm = (T: number, a: number, b: number) => states("T", repeat(T, n), "P", logGrid(a, b, n));
  const isobar = (p: number, a: number, b: number) => states("T", linGrid(a, b, n), "P", repeat(p, n));
  // The critical isobar from T_c to `b`, closing in on T_c: there c_p grows without bound and the isobar lies flat on
  // T–s, so a first leg a hundred and sixtieth of the range long cut across its bend, and Nitrogen's supercritical gas
  // took in states above p_c (PLAN.md M23.2).
  const criticalIsobar = (b: number) => states("T", awayFrom(Tc, b, n), "P", repeat(pc, n));
  const kept = (s: readonly (Corner | null)[]) => s.filter((c): c is Corner => c !== null);
  // A leg from the critical point leaves out its first cell, the critical point by (T, p), which the package's
  // critical point by (T, ρ) replaces; a leg onto the saturation curve leaves out its last.
  const fromCritical = (s: (Corner | null)[]) => s.slice(1);
  // Pressures from `a` onto the saturation pressure `b`, short of it: a state there is on the curve, and either phase.
  const toSaturation = (a: number, b: number, rows: number) => logGrid(a, b, rows).slice(0, -1);
  // The cold edge over `pressures`: on each isobar the lowest temperature at which the package has a state, to within
  // (T_c − T)/2^HALVINGS: the isotherm at the lowest T where the package has it, the melting line where it does not.
  // An isobar with no state below T_c has no vertex.
  const cold = async (pressures: number[]): Promise<(Corner | null)[]> => {
    const found = await states("T", repeat(low, pressures.length), "P", pressures);
    const open = pressures.flatMap((_, i) => (found[i] ? [] : [i]));
    const bounds = open.map(() => [low, Tc]);
    // Stryker disable next-line ConditionalExpression,LogicalOperator,EqualityOperator: with no isobar open, a halving asks for nothing.
    for (let k = 0; k < HALVINGS && open.length > 0; k++) {
      const mids = bounds.map(([a, b]) => ((a as number) + (b as number)) / 2);
      const got = await states(
        "T",
        mids,
        "P",
        open.map((i) => pressures[i] as number),
      );
      got.forEach((c, j) => {
        const bound = bounds[j] as number[];
        if (c) [bound[1], found[open[j] as number]] = [mids[j] as number, c];
        else bound[0] = mids[j] as number;
      });
    }
    return found;
  };

  const [critical] = (await states("T", [Tc], "Dmass", [l.rhoc])) as [Corner | null];
  const temperatures = domeTemperatures(low, Tc, 120);
  const liquid = await states("T", temperatures, "Q", repeat(0, temperatures.length));
  const vapour = await states("T", temperatures, "Q", repeat(1, temperatures.length));
  const saturated = liquid[0];
  if (!saturated) return { regions: [], isotherm: [], isobar: [], critical, Tc, pc };
  const floor = Math.min(ranges(l).P[0], saturated.P / 2);
  const top = l.pmax;

  const tcDown = fromCritical(await isotherm(Tc, pc, floor));
  const tcUp = fromCritical(await isotherm(Tc, pc, top));
  const pcLeft = fromCritical(await criticalIsobar(low));
  const pcRight = fromCritical(await criticalIsobar(high));
  const hotDown = await isotherm(high, pc, floor);
  const hotUp = await isotherm(high, pc, top);
  const topLeft = await isobar(top, Tc, low);
  const topRight = await isobar(top, Tc, high);
  const floorLeft = await isobar(floor, Tc, low);
  const floorRight = await isobar(floor, Tc, high);
  const coldHigh = await cold(logGrid(top, pc, COLD_ROWS));
  const coldLow = await cold(toSaturation(pc, saturated.P, COLD_ROWS));
  const coldGas = await states("T", repeat(low, n - 1), "P", toSaturation(floor, saturated.P, n));

  const outlines: [string, number, string, number, (Corner | null)[]][] = [];
  const middle = liquid[Math.floor(liquid.length / 2)];
  const [warm, mean] = [(Tc + high) / 2, Math.sqrt(pc * top)];
  outlines.push(
    ["T", warm, "P", mean, [critical, ...pcRight, ...hotUp, ...[...topRight].reverse(), ...[...tcUp].reverse()]],
    [
      "T",
      warm,
      "P",
      Math.sqrt(floor * pc),
      [critical, ...tcDown, ...floorRight, ...[...hotDown].reverse(), ...[...pcRight].reverse()],
    ],
    ["T", (low + Tc) / 2, "P", mean, [critical, ...tcUp, ...topLeft, ...coldHigh, ...[...pcLeft].reverse()]],
  );
  if (middle) {
    const [Tm, pm] = [middle.T, middle.P];
    outlines.push(
      ["T", Tm, "P", Math.sqrt(pm * pc), [...liquid, critical, ...pcLeft, ...coldLow]],
      ["T", Tm, "P", Math.sqrt(floor * pm), [...vapour, critical, ...tcDown, ...floorLeft, ...coldGas]],
      ["T", Tm, "Q", 0.5, [...liquid, critical, ...[...vapour].reverse()]],
    );
  }
  const regions: Region[] = [];
  for (const [name1, value1, name2, value2, outline] of outlines) {
    const phase = await phaseAt(name1, value1, name2, value2);
    if (phase !== null) regions.push({ phase, outline: kept(outline) });
  }
  return {
    regions,
    isotherm: runs([...[...tcDown].reverse(), critical, ...tcUp]),
    isobar: runs([...[...pcLeft].reverse(), critical, ...pcRight]),
    critical,
    Tc,
    pc,
  };
}

// A diagram's geometry and the phase regions as the page draws them (PLAN.md M21.6): the package computes them
// (phasekit-diagram, through `Fluid.diagram` and `Fluid.regions`, M19.6) and the engine hands over its typed arrays,
// which this reads into vertices, lines and corners. It adds no numerics: every number is the package's.
import type { CornersValue, DiagramValue, LinesValue, RegionsValue, VerticesValue } from "../engine/protocol";
import type { Key } from "./planes";

/** A point of a line: the package's x and y at its inputs, `[name1, value1, name2, value2]`. */
export interface Vertex {
  x: number;
  y: number;
  inputs: readonly [string, number, string, number];
}

export interface Segment {
  twoPhase: boolean;
  vertices: Vertex[];
}

export interface Line {
  held: Key;
  value: number;
  segments: Segment[];
}

/** The saturated liquid and vapour, each up to the critical point, and the critical point; a mixture's bubble and dew
 *  points. */
export interface Dome {
  liquid: Vertex[];
  vapour: Vertex[];
  critical: Vertex | null;
}

export interface DiagramData {
  dome: Dome;
  lines: Line[];
  x: [number, number];
  y: [number, number];
  /** A mixture's (PLAN.md M21.5): its components, and how the package's trace of its envelope ended and why. */
  envelope?: EnvelopeInfo;
}

export interface EnvelopeInfo {
  components: string[];
  /** The feed's mole fractions. */
  feed: number[];
  end: string;
  reason: string;
}

/** The keys every corner carries, in the package's order: each diagram's axes. */
export const AXES = ["T", "P", "Hmass", "Smass", "Dmass"] as const;
export type AxisKey = (typeof AXES)[number];

/** A state on a region's edge, by its value of each axis key. */
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
  /** The critical point, the package's state at (T_c, ρ_c), where it has one. */
  critical: Corner | null;
  /** The fluid's critical temperature and pressure, the package's constants. */
  Tc: number;
  pc: number;
}

/** The vertices, each with its pair's keys from `pairKeys`. */
export function vertices(v: VerticesValue, pairKeys: readonly string[]): Vertex[] {
  return Array.from(v.pairs, (pair, k) => ({
    x: v.xy[2 * k] as number,
    y: v.xy[2 * k + 1] as number,
    inputs: [
      pairKeys[2 * pair] as string,
      v.inputs[2 * k] as number,
      pairKeys[2 * pair + 1] as string,
      v.inputs[2 * k + 1] as number,
    ],
  }));
}

/** Each line, its segments, and each segment's run of vertices. */
function lines(l: LinesValue, pairKeys: readonly string[]): Line[] {
  const all = vertices(l.vertices, pairKeys);
  return l.held.map((held, i) => {
    const segments: Segment[] = [];
    for (let j = l.segments[i] as number; j < (l.segments[i + 1] as number); j++)
      segments.push({ twoPhase: l.twoPhase[j] === 1, vertices: all.slice(l.starts[j], l.starts[j + 1]) });
    return { held: held as Key, value: l.values[i] as number, segments };
  });
}

/** The package's diagram as the page draws it. */
export function readDiagram(d: DiagramValue): DiagramData {
  const data: DiagramData = {
    dome: {
      liquid: vertices(d.liquid, d.pairKeys),
      vapour: vertices(d.vapour, d.pairKeys),
      critical: vertices(d.critical, d.pairKeys)[0] ?? null,
    },
    lines: lines(d.lines, d.pairKeys),
    x: [d.x[0] as number, d.x[1] as number],
    y: [d.y[0] as number, d.y[1] as number],
  };
  if (d.mixture) data.envelope = d.mixture;
  return data;
}

/** The corner whose five values start at `at`. */
function corner(values: Float64Array, at: number): Corner {
  return Object.fromEntries(AXES.map((k, i) => [k, values[at + i]])) as Corner;
}

/** Each run of corners. */
function runs(c: CornersValue): Corner[][] {
  return Array.from({ length: c.starts.length - 1 }, (_, i) =>
    Array.from({ length: (c.starts[i + 1] as number) - (c.starts[i] as number) }, (_, k) =>
      corner(c.corners, AXES.length * ((c.starts[i] as number) + k)),
    ),
  );
}

/** The package's phase regions as the page draws them. */
export function readRegions(r: RegionsValue): Regions {
  const outlines = runs(r.outlines);
  return {
    regions: r.phases.map((phase, i) => ({ phase, outline: outlines[i] as Corner[] })),
    isotherm: runs(r.isotherm),
    isobar: runs(r.isobar),
    critical: r.critical.length === AXES.length ? corner(r.critical, 0) : null,
    Tc: r.Tc,
    pc: r.pc,
  };
}

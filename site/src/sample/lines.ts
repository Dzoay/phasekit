// A diagram's geometry (PLAN.md M21.2): the saturation dome, the critical point and each family's isolines, sampled by
// the package's batch. It adds no numerics: every vertex is the package's value at the inputs it carries. A line meets
// the dome at saturation states the package computes (QT, PQ, QS, HQ, DQ), its two-phase part between them; a failed
// cell breaks a line. M21.6 replaces this module with M19's geometry.
import type { BatchValue } from "../engine/protocol";
import type { Diagram, Family, Key } from "./diagrams";
import { domeTemperatures, linGrid, logGrid, ranges, values, type Limits } from "./ranges";

/** The package's batch over one fluid: the page's worker in the browser, the handler itself in tests. */
export type Batch = (
  outputs: string[],
  name1: string,
  values1: Float64Array,
  name2: string,
  values2: Float64Array,
) => Promise<BatchValue>;

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

/** The saturated liquid and vapour, each up to just below T_c, and the critical point. */
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
}

/** The highest cell code that is a value: 0 success, 1 a flagged extrapolation (PLAN.md M10.8). */
const VALUE = 1;

/** Points along a line, in its sweep's order: a vertex on one side of the dome or inside it, or a saturation state on
 *  its edge; or null, a failed cell. */
export type Item = { vertex: Vertex; twoPhase: boolean } | { edge: Vertex } | null;

export interface Row {
  cells: number[];
  ok: boolean[];
  inputs: Vertex["inputs"];
}

async function rows(
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

/** The row's vertex: x and y are its first two outputs; null where either failed. */
function vertex(row: Row): Vertex | null {
  return row.ok[0] && row.ok[1] ? { x: row.cells[0] as number, y: row.cells[1] as number, inputs: row.inputs } : null;
}

/** Segments from items: a failed cell ends one, a change of side starts one, and an edge vertex ends one and starts
 *  the next, so a line stays continuous through the dome's edge. A lone vertex draws nothing, so it is dropped. */
export function segments(items: readonly Item[]): Segment[] {
  const out: Segment[] = [];
  let current: { twoPhase: boolean | null; vertices: Vertex[] } = { twoPhase: null, vertices: [] };
  const close = () => {
    if (current.vertices.length > 1) out.push({ twoPhase: current.twoPhase ?? false, vertices: current.vertices });
  };
  for (const item of items) {
    if (item === null) {
      close();
      current = { twoPhase: null, vertices: [] };
    } else if ("edge" in item) {
      current.vertices.push(item.edge);
      close();
      current = { twoPhase: null, vertices: [item.edge] };
    } else {
      const { twoPhase } = item;
      if (current.twoPhase !== null && current.twoPhase !== twoPhase) {
        close();
        current = { twoPhase: null, vertices: [] };
      }
      current.twoPhase = twoPhase;
      current.vertices.push(item.vertex);
    }
  }
  close();
  return out;
}

const repeat = (v: number, n: number): number[] => Array.from({ length: n }, () => v);

/** A line of constant T or p: one-phase on either side of the saturation state at its value, and between them the
 *  two-phase part from QT or PQ, entered from the side the sweep comes from (the vapour for an isotherm swept up in p,
 *  the liquid for an isobar swept up in T). Above the critical point there is no dome to cross. */
async function heldTP(d: Diagram, f: Family, v: number, sweep: number[], batch: Batch): Promise<Segment[]> {
  const [x, y] = [d.x.key, d.y.key];
  const swept = await rows(batch, [x, y], f.held, repeat(v, sweep.length), f.sweep, sweep);
  const qs = linGrid(0, 1, 11);
  const sat = await rows(batch, [x, y, f.sweep], f.held, repeat(v, qs.length), "Q", qs);
  const one = (r: Row): Item => {
    const at = vertex(r);
    return at && { vertex: at, twoPhase: false };
  };
  if (!sat.every((r) => r.ok.every(Boolean))) return segments(swept.map(one));
  const crossing = sat[0]?.cells[2] as number;
  const inside = (f.held === "T" ? [...sat].reverse() : sat).map((r, i, all): Item => {
    const at = vertex(r) as Vertex;
    return i === 0 || i === all.length - 1 ? { edge: at } : { vertex: at, twoPhase: true };
  });
  // Stryker disable next-line EqualityOperator: no grid point lands exactly on a saturation pressure or temperature.
  const side = (below: boolean) => swept.filter((_, i) => (sweep[i] as number) < crossing === below).map(one);
  return segments([...side(true), ...inside, ...side(false)]);
}

/** A line of constant s, h or ρ: the package's states along the sweep, two-phase where the batch's Q is defined, with
 *  the saturation states at its value (QS, HQ, DQ) where it meets the dome. */
async function heldOther(d: Diagram, f: Family, v: number, sweep: number[], batch: Batch): Promise<Segment[]> {
  const [x, y] = [d.x.key, d.y.key];
  const swept = await rows(batch, [x, y, "Q"], f.held, repeat(v, sweep.length), f.sweep, sweep);
  const sat = await rows(batch, [x, y, f.sweep], f.held, [v, v], "Q", [0, 1]);
  const items: { at: number; item: Item }[] = swept.map((r, i) => {
    const at = vertex(r);
    return { at: sweep[i] as number, item: at && { vertex: at, twoPhase: Boolean(r.ok[2]) } };
  });
  for (const r of sat) {
    const at = vertex(r);
    if (at && r.ok[2]) items.push({ at: r.cells[2] as number, item: { edge: at } });
  }
  return segments(items.sort((a, b) => a.at - b.at).map((i) => i.item));
}

/** A line of constant quality, across the dome's temperatures. */
async function heldQ(d: Diagram, v: number, temperatures: number[], batch: Batch): Promise<Segment[]> {
  const found = await rows(batch, [d.x.key, d.y.key], "T", temperatures, "Q", repeat(v, temperatures.length));
  return segments(
    found.map((r): Item => {
      const at = vertex(r);
      return at && { vertex: at, twoPhase: true };
    }),
  );
}

const extentKeys = ["Hmass", "Smass", "Dmass"] as const;

/** The least and greatest of each key's values that the package gave, the keys' cells from `offset` on. */
export function extents(found: readonly Row[], offset: number): Record<(typeof extentKeys)[number], [number, number]> {
  const span = (k: number): [number, number] => {
    const all = found.filter((r) => r.ok[offset + k]).map((r) => r.cells[offset + k] as number);
    return [Math.min(...all), Math.max(...all)];
  };
  return { Hmass: span(0), Smass: span(1), Dmass: span(2) };
}

/** The diagram's dome, critical point and isolines for a fluid, `n` points to a swept line. */
export async function sampleDiagram(d: Diagram, l: Limits, batch: Batch, n = 160): Promise<DiagramData> {
  const r = ranges(l);
  const [x, y] = [d.x.key, d.y.key];
  const outputs = [x, y, ...extentKeys];
  // The dome starts where it enters the plotted pressures: at the saturation temperature of the lowest one, or at the
  // lowest temperature if that is higher.
  const lowest = (await rows(batch, ["T"], "P", [r.P[0]], "Q", [0]))[0] as Row;
  const start = lowest.ok[0] ? Math.max(r.T[0], lowest.cells[0] as number) : r.T[0];
  const temperatures = domeTemperatures(start, l.Tc, 120);
  const liquid = await rows(batch, outputs, "T", temperatures, "Q", repeat(0, temperatures.length));
  const vapour = await rows(batch, outputs, "T", temperatures, "Q", repeat(1, temperatures.length));
  const [critical] = await rows(batch, outputs, "T", [l.Tc], "Dmass", [l.rhoc]);
  const corners = await rows(batch, outputs, "T", [r.T[0], r.T[0], r.T[1], r.T[1]], "P", [
    r.P[0],
    r.P[1],
    r.P[0],
    r.P[1],
  ]);
  const span = { T: r.T, P: r.P, Q: [0, 1] as [number, number], ...extents([...liquid, ...vapour, ...corners], 2) };
  const sweeps = { T: linGrid(r.T[0], r.T[1], n), P: logGrid(r.P[0], r.P[1], n) };
  const lines: Line[] = [];
  for (const f of d.families) {
    for (const v of values(f.held, span[f.held])) {
      const segs =
        f.held === "Q"
          ? await heldQ(d, v, temperatures, batch)
          : f.held === "T" || f.held === "P"
            ? await heldTP(d, f, v, sweeps[f.sweep], batch)
            : await heldOther(d, f, v, sweeps[f.sweep], batch);
      lines.push({ held: f.held, value: v, segments: segs });
    }
  }
  const dome: Dome = {
    liquid: liquid.map(vertex).filter((v): v is Vertex => v !== null),
    vapour: vapour.map(vertex).filter((v): v is Vertex => v !== null),
    critical: critical ? vertex(critical) : null,
  };
  return { dome, lines, x: span[x], y: span[y] };
}

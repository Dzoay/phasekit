// The benchmark's comparison of the two builds (PLAN.md M21.3; user decision SL8: both builds give the same bits): cell
// by cell, each value's 64 bits and each status code, and the cells by status.
import type { BatchValue } from "../engine/protocol";

export interface Difference {
  cell: number;
  a: number;
  b: number;
}

export interface Comparison {
  cells: number;
  /** The cells whose values differ in any bit, or whose status codes differ. */
  differing: number;
  first: Difference | null;
}

/** The two answers, cell by cell; answers of different sizes differ in every cell of the larger. */
export function compareBuilds(a: BatchValue, b: BatchValue): Comparison {
  const cells = Math.max(a.values.length, b.values.length);
  const bits = (v: Float64Array) => new BigUint64Array(v.buffer, v.byteOffset, v.length);
  const [x, y] = [bits(a.values), bits(b.values)];
  let differing = 0;
  let first: Difference | null = null;
  for (let i = 0; i < cells; i++) {
    if (x[i] !== y[i] || a.status[i] !== b.status[i]) {
      differing += 1;
      first ??= { cell: i, a: a.values[i] ?? NaN, b: b.values[i] ?? NaN };
    }
  }
  return { cells, differing, first };
}

/** How many cells have each status: 0 a value, 1 a flagged extrapolation, the failures by their code. */
export function byStatus(status: Uint8Array): Map<number, number> {
  const counts = new Map<number, number>();
  for (const code of status) counts.set(code, (counts.get(code) ?? 0) + 1);
  return counts;
}

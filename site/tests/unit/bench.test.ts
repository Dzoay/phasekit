// The benchmark (PLAN.md M21.3) under Node: its grids, the engine's timed batch, and its comparison of the two builds,
// which finds a planted one-ulp difference and none between the builds on n-Propane's DT, QT and PQ grids (user
// decision SL8: the simd128 build's cells are the baseline's, bit for bit).
import { describe, expect, test } from "vitest";
import { BENCH_OUTPUTS, PAIRS, benchGrid } from "../../src/bench/grids";
import { byStatus, compareBuilds } from "../../src/bench/compare";
import { createHandler } from "../../src/engine/handler";
import type { BatchValue, Request } from "../../src/engine/protocol";
import { LIMIT_KEYS, limits, ranges, type Limits } from "../../src/lib/ranges";
import { loadPk } from "../pk";

async function handlers() {
  const simd = createHandler(await loadPk("simd"), "simd128");
  const baseline = createHandler(await loadPk("baseline"), "baseline");
  const c = simd({ id: 1, op: "constants", fluid: "n-Propane", keys: [...LIMIT_KEYS] });
  return { simd, baseline, l: limits(c.ok && c.op === "constants" ? c.value : []) as Limits };
}

describe("the benchmark's grids", () => {
  test("are n states of the pair, a near-square grid over the plotted ranges", async () => {
    const { l } = await handlers();
    const r = ranges(l);
    expect(PAIRS.map((p) => p.id)).toEqual(["DT", "PT", "QT", "PQ"]);
    const dt = benchGrid("DT", l, 10);
    expect([dt.name1, dt.name2, dt.values1.length, dt.values2.length]).toEqual(["T", "Dmass", 10, 10]);
    // A side of 4: T steps along the row, ρ down the rows.
    expect([...dt.values1.slice(0, 5)]).toEqual([r.T[0], dt.values1[1], dt.values1[2], r.T[1], r.T[0]]);
    expect(dt.values2[0]).toBeCloseTo(l.rhoc / 1000, 12);
    expect([dt.values2[3], dt.values2[4]]).toEqual([dt.values2[0], dt.values2[5]]);
    expect(benchGrid("DT", l, 16).values2[15]).toBe(3 * l.rhoc);
    const pt = benchGrid("PT", l, 4);
    expect([pt.name1, ...pt.values1, pt.name2, ...pt.values2]).toEqual([
      "T",
      r.T[0],
      r.T[1],
      r.T[0],
      r.T[1],
      "P",
      r.P[0],
      r.P[0],
      r.P[1],
      r.P[1],
    ]);
    const qt = benchGrid("QT", l, 4);
    expect([qt.name1, ...qt.values1, qt.name2, ...qt.values2]).toEqual([
      "T",
      r.T[0],
      l.Tc * 0.999,
      r.T[0],
      l.Tc * 0.999,
      "Q",
      0,
      0,
      1,
      1,
    ]);
    const pq = benchGrid("PQ", l, 4);
    expect([pq.name1, ...pq.values1, pq.name2, ...pq.values2]).toEqual([
      "P",
      r.P[0],
      l.pc * 0.999,
      r.P[0],
      l.pc * 0.999,
      "Q",
      0,
      0,
      1,
      1,
    ]);
    // One state still has a side of two.
    expect(benchGrid("PQ", l, 1).values1).toEqual(Float64Array.from([r.P[0]]));
  });
});

describe("the benchmark's comparison", () => {
  const answer = (values: number[], status: number[]): BatchValue => ({
    values: Float64Array.from(values),
    status: Uint8Array.from(status),
    outputs: 1,
  });

  test("finds a planted one-ulp difference, a differing status, and nothing between equal answers", () => {
    const a = answer([1, 2, 3, 0.1], [0, 0, 0, 0]);
    expect(compareBuilds(a, answer([1, 2, 3, 0.1], [0, 0, 0, 0]))).toEqual({ cells: 4, differing: 0, first: null });
    const ulp = 3 + 2 ** -51;
    expect(compareBuilds(a, answer([1, 2, ulp, 0.1], [0, 0, 0, 0]))).toEqual({
      cells: 4,
      differing: 1,
      first: { cell: 2, a: 3, b: ulp },
    });
    expect(compareBuilds(a, answer([1, 2, 3, 0.1], [0, 4, 0, 4])).differing).toBe(2);
    // −0 and +0 differ in their sign bit.
    expect(compareBuilds(answer([0], [0]), answer([-0], [0])).differing).toBe(1);
    // A shorter answer differs in every cell it lacks.
    expect(compareBuilds(a, answer([1, 2], [0, 0]))).toEqual({
      cells: 4,
      differing: 2,
      first: { cell: 2, a: 3, b: NaN },
    });
  });

  test("counts the cells by status", () => {
    expect([...byStatus(Uint8Array.from([0, 0, 4, 1, 0, 4]))]).toEqual([
      [0, 3],
      [4, 2],
      [1, 1],
    ]);
  });

  test("finds the two builds equal on n-Propane's DT, QT and PQ grids, each timed", { timeout: 60_000 }, async () => {
    const { simd, baseline, l } = await handlers();
    for (const pair of ["DT", "QT", "PQ"] as const) {
      const grid = benchGrid(pair, l, 400);
      const run = (handle: typeof simd) => {
        const r = handle({ id: 2, op: "bench", fluid: "n-Propane", outputs: [...BENCH_OUTPUTS], ...grid } as Request);
        if (!r.ok || r.op !== "bench") throw new Error(r.ok ? r.op : r.error);
        return r.value;
      };
      const [a, b] = [run(baseline), run(simd)];
      expect(compareBuilds(a.batch, b.batch), pair).toEqual({
        cells: 400 * BENCH_OUTPUTS.length,
        differing: 0,
        first: null,
      });
      expect(a.batch.outputs).toBe(BENCH_OUTPUTS.length);
      // Each run is the batch: milliseconds, so a few runs fill the 20 ms budget, not the million an empty one would.
      expect(a.timing.runs).toBeGreaterThan(0);
      expect(a.timing.runs).toBeLessThan(1000);
      expect(b.timing.us).toBeGreaterThan(0);
      expect((byStatus(a.batch.status).get(0) ?? 0) > 0, pair).toBe(true);
    }
  });
});

// The phase regions (PLAN.md M21.9) under Node on the real package: inside each region and clear of its edges, the
// package's phase by the diagram's readout pair is the region's; each region's label sits inside it, at a state of its
// phase; the critical isotherm and isobar run through the package's critical point.
import { describe, expect, test } from "vitest";
import { buildModel, type Point } from "../../src/diagram/model";
import { baseScales, readoutAt } from "../../src/diagram/scales";
import { createHandler } from "../../src/engine/handler";
import type { Request, Response } from "../../src/engine/protocol";
import { DIAGRAMS, type Diagram } from "../../src/sample/diagrams";
import type { Batch, DiagramData } from "../../src/sample/lines";
import { LIMIT_KEYS, limits, logGrid, ranges, type Limits } from "../../src/sample/ranges";
import {
  COLD_ROWS,
  runs,
  sampleRegions,
  type AxisKey,
  type Corner,
  type PhaseAt,
  type Regions,
} from "../../src/sample/regions";
import { embedded, loadPk } from "../pk";

async function engine(fluid: string) {
  const handle = createHandler(await loadPk("simd"), "simd128");
  let id = 0;
  const call = (body: Record<string, unknown>): Response => handle({ ...body, fluid, id: ++id } as Request);
  const batch: Batch = (outputs, name1, values1, name2, values2) => {
    const r = call({ op: "batch", outputs, name1, values1, name2, values2 });
    return r.ok && r.op === "batch" ? Promise.resolve(r.value) : Promise.reject(new Error("refused"));
  };
  const phaseAt: PhaseAt = (name1, value1, name2, value2) => {
    const r = call({ op: "state", name1, value1, name2, value2, outputs: [] });
    return Promise.resolve(r.ok && r.op === "state" ? r.value.phase : null);
  };
  const constants = call({ op: "constants", keys: [...LIMIT_KEYS] });
  const l = limits(constants.ok && constants.op === "constants" ? constants.value : []) as Limits;
  // The package's values of `outputs` at a state.
  const at = (outputs: string[], name1: string, value1: number, name2: string, value2: number) => {
    const r = call({ op: "state", name1, value1, name2, value2, outputs });
    return r.ok && r.op === "state" ? (r.value.values as number[]) : [];
  };
  return { batch, phaseAt, l, at };
}

/** The test's own plane geometry, which Stryker leaves uninstrumented (src/diagram/geometry.ts has its own tests):
 *  whether `p` is inside the polygon by the even-odd rule, and its distance to the polygon's nearest edge. */
function within(p: Point, polygon: readonly Point[]): boolean {
  let odd = false;
  for (let i = 0, j = polygon.length - 1; i < polygon.length; j = i++) {
    const a = polygon[i] as Point;
    const b = polygon[j] as Point;
    if (a[1] > p[1] !== b[1] > p[1] && p[0] < a[0] + ((b[0] - a[0]) * (p[1] - a[1])) / (b[1] - a[1])) odd = !odd;
  }
  return odd;
}

function depth(p: Point, polygon: readonly Point[]): number {
  let nearest = Infinity;
  for (let i = 0, j = polygon.length - 1; i < polygon.length; j = i++) {
    const a = polygon[j] as Point;
    const b = polygon[i] as Point;
    const [dx, dy] = [b[0] - a[0], b[1] - a[1]];
    const length = dx * dx + dy * dy;
    const t = length === 0 ? 0 : Math.min(1, Math.max(0, ((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / length));
    nearest = Math.min(nearest, Math.hypot(p[0] - a[0] - t * dx, p[1] - a[1] - t * dy));
  }
  return nearest;
}

/** The points of a `columns` × `rows` grid over [x0, x1] × [y0, y1], ends left out, inside the polygon, with depths. */
type Span = [number, number, number, number];

function grid(polygon: readonly Point[], [x0, x1, y0, y1]: Span, columns: number, rows: number) {
  const found: { p: Point; depth: number }[] = [];
  for (let i = 1; i < columns; i++) {
    for (let j = 1; j < rows; j++) {
      const p: Point = [x0 + ((x1 - x0) * i) / columns, y0 + ((y1 - y0) * j) / rows];
      if (within(p, polygon)) found.push({ p, depth: depth(p, polygon) });
    }
  }
  return found;
}

const ALL = ["liquid", "gas", "twophase", "supercritical", "supercritical_liquid", "supercritical_gas"];

/** A diagram's plotted box, in [0, 1]² (an axis's logarithm where it is logarithmic): the regions' corners inside the
 *  plotted ranges span it. */
function frame(d: Diagram, regions: Regions, l: Limits) {
  const r = ranges(l);
  const corners = regions.regions
    .flatMap((g) => g.outline)
    .filter((c) => r.T[0] <= c.T && c.T <= r.T[1] && r.P[0] <= c.P && c.P <= r.P[1]);
  const axis = (key: AxisKey, log: boolean) => {
    const u = (v: number) => (log ? Math.log10(v) : v);
    const all = corners.map((c) => u(c[key]));
    const [lo, hi] = [Math.min(...all), Math.max(...all)];
    return {
      to: (v: number) => (u(v) - lo) / (hi - lo),
      from: (t: number) => (log ? 10 ** (lo + t * (hi - lo)) : lo + t * (hi - lo)),
    };
  };
  const [x, y] = [axis(d.x.key as AxisKey, d.x.log), axis(d.y.key as AxisKey, d.y.log)];
  return { x, y, at: (c: Corner): Point => [x.to(c[d.x.key as AxisKey]), y.to(c[d.y.key as AxisKey])] };
}

const EMBEDDED = await embedded();

describe("the phase regions", { timeout: 60_000 }, () => {
  for (const fluid of EMBEDDED) {
    test(`of ${fluid}: inside each region, clear of its edges, the package's phase is the region's`, async () => {
      const { batch, phaseAt, l } = await engine(fluid);
      const sampled = await sampleRegions(l, batch, phaseAt);
      expect(sampled.regions.map((g) => g.phase).sort()).toEqual([...ALL].sort());
      for (const d of DIAGRAMS) {
        const f = frame(d, sampled, l);
        const seen = new Set<string>();
        for (const region of sampled.regions) {
          const outline = region.outline.map(f.at);
          // A coarse grid over the box, clear of the edges by 2 % of it; where that finds nothing, the deepest point of a
          // fine one over the region's part of the box: the compressed liquid is a sliver on T–s, T–ρ and h–s.
          const coarse = grid(outline, [0, 1, 0, 1], 40, 30).filter((g) => g.depth >= 0.02);
          const clamp = (v: number) => Math.min(1, Math.max(0, v));
          const [xs, ys] = [outline.map((q) => clamp(q[0])), outline.map((q) => clamp(q[1]))];
          const span: Span = [Math.min(...xs), Math.max(...xs), Math.min(...ys), Math.max(...ys)];
          const fine = () => grid(outline, span, 300, 300).sort((a, b) => b.depth - a.depth);
          const points = (coarse.length > 0 ? coarse : fine().slice(0, 1)).map((g) => g.p);
          // Up to eight of them, spread over the region.
          for (const [u, v] of points.filter((_, k) => k % Math.ceil(points.length / 8) === 0)) {
            const inputs = readoutAt(d, f.x.from(u), f.y.from(v), "si");
            expect(await phaseAt(...inputs), `${fluid} ${d.id} ${region.phase} at ${inputs.join(" ")}`).toBe(
              region.phase,
            );
            seen.add(region.phase);
          }
        }
        // On p–T the liquid + vapour region is the vapour-pressure curve, with nothing inside it.
        expect([...seen].sort(), `${fluid} ${d.id}`).toEqual(
          ALL.filter((p) => d.id !== "pt" || p !== "twophase").sort(),
        );
      }
    });
  }

  test("each region's label sits inside it, at a state of its phase", async () => {
    const { batch, phaseAt, l } = await engine("Water");
    const sampled = await sampleRegions(l, batch, phaseAt);
    const labelled = new Set<string>();
    for (const d of DIAGRAMS) {
      const f = frame(d, sampled, l);
      const data = {
        dome: { liquid: [], vapour: [], critical: null },
        lines: [],
        x: [f.x.from(0), f.x.from(1)],
        y: [f.y.from(0), f.y.from(1)],
      } as DiagramData;
      const scales = baseScales(d, data, 800, 500, "si");
      const model = buildModel(d, data, {
        ...scales,
        system: "si",
        hidden: new Set(),
        colour: (t) => t,
        regions: sampled,
      });
      expect(model.fills.map((g) => g.colour)).toEqual(
        sampled.regions.map((g) => `--phase-${g.phase.replace(/_/g, "-")}`),
      );
      for (const label of model.regionLabels) {
        const region = sampled.regions.find((g) => g.phase === label.phase) as Regions["regions"][number];
        const outline = region.outline.map((c): Point => [
          scales.x(c[d.x.key as AxisKey]),
          scales.y(c[d.y.key as AxisKey]),
        ]);
        expect(within([label.x, label.y], outline), `${d.id} ${label.phase}`).toBe(true);
        const inputs = readoutAt(d, scales.x.invert(label.x), scales.y.invert(label.y), "si");
        expect(await phaseAt(...inputs), `${d.id} ${label.phase}`).toBe(label.phase);
        labelled.add(label.phase);
      }
    }
    expect([...labelled].sort()).toEqual([...ALL].sort());
  });

  test("each region's phase is the package's at a state inside it", async () => {
    const { batch, phaseAt, l, at } = await engine("Water");
    const seeds: [string, number, string, number][] = [];
    const sampled = await sampleRegions(l, batch, (...inputs) => {
      seeds.push(inputs);
      return phaseAt(...inputs);
    });
    expect(seeds.length).toBe(6);
    // On p–h, in h and log p, where every region has room.
    const ph = (c: Corner): Point => [c.Hmass, Math.log10(c.P)];
    sampled.regions.forEach((region, i) => {
      const [h, p] = at(["Hmass", "P"], ...(seeds[i] as [string, number, string, number]));
      expect(within([h as number, Math.log10(p as number)], region.outline.map(ph)), region.phase).toBe(true);
    });
  });

  test("each outline walks its region's edge, a step between corners at most a tenth of the range", async () => {
    // In T and ln p, each over its whole span. (Where a fluid's melting line crosses T_c, as Nitrogen's does, an edge
    // cuts across the states the package refuses there, in one longer step.)
    for (const fluid of ["Water", "n-Propane"]) {
      const { batch, phaseAt, l } = await engine(fluid);
      const sampled = await sampleRegions(l, batch, phaseAt);
      const low = ranges(l).T[0];
      const floor = Math.min(...sampled.regions.flatMap((g) => g.outline.map((c) => c.P)));
      const u = (c: Corner): Point => [(c.T - low) / (l.Tmax - low), Math.log(c.P / floor) / Math.log(l.pmax / floor)];
      for (const region of sampled.regions) {
        const steps = region.outline.map((c, i) => {
          const [a, b] = [u(region.outline.at(i - 1) as Corner), u(c)];
          return Math.hypot(a[0] - b[0], a[1] - b[1]);
        });
        expect(Math.max(...steps), `${fluid} ${region.phase}`).toBeLessThan(0.1);
      }
    }
  });

  test("the cold edge halves only the isobars the package refused at the lowest T", async () => {
    const { batch, phaseAt, l } = await engine("Water");
    const low = ranges(l).T[0];
    const asked: { values1: number[]; values2: number[]; refused: number[] }[] = [];
    await sampleRegions(
      l,
      async (outputs, name1, values1, name2, values2) => {
        const answer = await batch(outputs, name1, values1, name2, values2);
        if (name1 === "T" && name2 === "P") {
          const refused = [...values2].filter((_, i) =>
            outputs.some((_, k) => (answer.status[i * outputs.length + k] as number) > 1),
          );
          asked.push({ values1: [...values1], values2: [...values2], refused });
        }
        return answer;
      },
      phaseAt,
    );
    // Water's isotherm at its triple point reaches ice above about 0.6 GPa: those isobars, and only they, are halved.
    const first = asked.findIndex((a) => a.values2.length === COLD_ROWS && a.values1.every((t) => t === low));
    const refused = (asked[first] as { refused: number[] }).refused;
    expect(refused.length).toBeGreaterThan(0);
    expect(refused.length).toBeLessThan(COLD_ROWS);
    expect(asked[first + 1]?.values2).toEqual(refused);
    expect(asked[first + 1]?.values1.every((t) => t > low)).toBe(true);
  });

  test("the cold edge follows the melting line, by bisection on each isobar", async () => {
    const { batch, phaseAt, l } = await engine("n-Propane");
    const sampled = await sampleRegions(l, batch, phaseAt);
    // The isotherm at n-Propane's lowest T is past its melting line at every pressure above p_c: each cold isobar
    // there has its state at a higher T.
    const outline = sampled.regions.find((g) => g.phase === "supercritical_liquid")?.outline ?? [];
    const low = ranges(l).T[0];
    for (const p of logGrid(l.pmax, l.pc, COLD_ROWS)) {
      const on = outline.filter((c) => Math.abs(c.P / p - 1) < 1e-9 && c.T < l.Tc);
      expect(on.length, `${p} Pa`).toBeGreaterThan(0);
      expect(Math.min(...on.map((c) => c.T)), `${p} Pa`).toBeGreaterThan(low);
    }
  });

  test("a leg onto the saturation curve stops a cell short of it", async () => {
    const { batch, phaseAt, l, at } = await engine("Water");
    const asked: { values1: number[]; values2: number[] }[] = [];
    const low = ranges(l).T[0];
    await sampleRegions(
      l,
      (outputs, name1, values1, name2, values2) => {
        if (name1 === "T" && name2 === "P") asked.push({ values1: [...values1], values2: [...values2] });
        return batch(outputs, name1, values1, name2, values2);
      },
      phaseAt,
    );
    const [saturated] = at(["P"], "T", low, "Q", 0) as [number];
    const cold = asked.filter((a) => a.values1.every((t) => t === low));
    // The gas side's isotherm: 159 cells from below, the last one log step under the saturation pressure.
    const gas = cold.find((a) => a.values2.length === 159)?.values2 ?? [];
    expect(Math.max(...gas)).toBeLessThan(saturated * (1 - 1e-9));
    expect(Math.max(...gas)).toBeGreaterThan(saturated * 0.99);
    // The liquid side's cold edge: its first COLD_ROWS - 1 cells from p_c down, the last a step over it.
    const liquid = cold.find((a) => a.values2.length === COLD_ROWS - 1)?.values2 ?? [];
    expect(Math.min(...liquid)).toBeGreaterThan(saturated * (1 + 1e-9));
    expect(Math.min(...liquid)).toBeLessThan(saturated * 1.5);
  });

  test("the critical isotherm and isobar run through the package's critical point", async () => {
    const { batch, phaseAt, l } = await engine("n-Propane");
    const { isotherm, isobar, critical } = await sampleRegions(l, batch, phaseAt);
    expect(Math.abs((critical?.T ?? 0) / l.Tc - 1)).toBeLessThan(1e-9);
    for (const [line, key, value] of [
      [isotherm, "T", l.Tc],
      [isobar, "P", l.pc],
    ] as const) {
      expect(line.flat()).toContain(critical);
      // In order along the line: p up the isotherm, T along the isobar.
      const along = line.flat().map((c) => c[key === "T" ? "P" : "T"]);
      expect(along.every((v, i) => i === 0 || v > (along[i - 1] as number))).toBe(true);
      for (const c of line.flat().filter((c) => c !== critical))
        expect(Math.abs(c[key] / value - 1)).toBeLessThan(1e-9);
      const [lo, hi] = [
        Math.min(...line.flat().map((c) => c[key === "T" ? "P" : "T"])),
        Math.max(...line.flat().map((c) => c[key === "T" ? "P" : "T"])),
      ];
      expect(lo).toBeLessThan(key === "T" ? l.pc / 1e3 : l.Tc * 0.5);
      expect(hi).toBeGreaterThan(key === "T" ? l.pc * 10 : l.Tc * 1.5);
    }
  });
});

describe("a region's edges", () => {
  const c = (T: number): Corner => ({ T, P: 1, Hmass: 0, Smass: 0, Dmass: 0 });

  test("break where a cell failed, and a run of one draws nothing", () => {
    expect(runs([c(1), c(2), null, c(3), null, c(4), c(5), c(6)])).toEqual([
      [c(1), c(2)],
      [c(4), c(5), c(6)],
    ]);
    expect(runs([null, c(1), null])).toEqual([]);
  });

  test("leave out a region whose phase the package refuses, and join across a critical point it refuses", async () => {
    const { batch, l } = await engine("Water");
    const refused = await sampleRegions(l, batch, (n1, v1, n2) => Promise.resolve(n2 === "Q" ? null : "x"));
    expect(refused.regions.map((g) => g.phase)).toEqual(["x", "x", "x", "x", "x"]);
    // Nitrogen's critical point by (T_c, ρ_c) is mechanically unstable to the package: its regions join across it,
    // its critical lines break there, and they keep the constants' T_c and p_c.
    const n2 = await engine("Nitrogen");
    const nitrogen = await sampleRegions(n2.l, n2.batch, n2.phaseAt);
    expect(nitrogen.critical).toBeNull();
    expect(nitrogen.regions.map((g) => g.phase).sort()).toEqual([...ALL].sort());
    expect([nitrogen.isotherm.length, nitrogen.isobar.length]).toEqual([2, 2]);
    expect([nitrogen.Tc, nitrogen.pc]).toEqual([n2.l.Tc, n2.l.pc]);
  });

  test("leave out a state whose any output failed, and the regions under the dome without its middle", async () => {
    const { batch, l } = await engine("Water");
    const failing =
      (fail: (name2: string, values2: Float64Array, i: number) => boolean): Batch =>
      async (outputs, name1, values1, name2, values2) => {
        const answer = await batch(outputs, name1, values1, name2, values2);
        const status = Uint8Array.from(answer.status);
        values1.forEach((_, i) => {
          if (fail(name2, values2, i)) status[i * outputs.length + 2] = 4;
        });
        return { ...answer, status };
      };
    // The critical point's h fails: the point is left out.
    const partial = await sampleRegions(
      l,
      failing((name2) => name2 === "Dmass"),
      () => Promise.resolve("x"),
    );
    expect(partial.critical).toBeNull();
    expect(partial.regions.length).toBe(6);
    // The saturated liquid's middle state fails: only the three regions above the critical point remain.
    const middle = await sampleRegions(
      l,
      failing((name2, values2, i) => name2 === "Q" && values2.length === 120 && values2[0] === 0 && i === 60),
      () => Promise.resolve("x"),
    );
    expect(middle.regions.length).toBe(3);
  });

  test("draw nothing where the package has no saturated liquid at the lowest temperature", async () => {
    const { batch, l } = await engine("Water");
    const none = await sampleRegions({ ...l, Tmin: 200, Ttriple: 200 }, batch, () => Promise.resolve("x"));
    expect(none).toEqual({ regions: [], isotherm: [], isobar: [], critical: none.critical, Tc: l.Tc, pc: l.pc });
    expect(none.critical?.T).toBeCloseTo(l.Tc, 6);
  });
});

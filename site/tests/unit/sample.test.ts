// The diagrams' sampling (PLAN.md M21.2) under Node on the real package: it adds no numerics, so every vertex is the
// package's value at that vertex's inputs (`Exact`); a one-phase segment lies on one side of the dome, a two-phase one
// ends on the saturation states at its line's value; a failed cell leaves a gap.
import { describe, expect, test } from "vitest";
import { createHandler } from "../../src/engine/handler";
import type { BatchValue, Request } from "../../src/engine/protocol";
import { DIAGRAMS, diagram, type Diagram } from "../../src/sample/diagrams";
import {
  extents,
  sampleDiagram,
  segments,
  type Batch,
  type DiagramData,
  type Row,
  type Vertex,
} from "../../src/sample/lines";
import {
  LIMIT_KEYS,
  domeTemperatures,
  limits,
  linGrid,
  logGrid,
  ranges,
  values,
  type Limits,
} from "../../src/sample/ranges";
import { loadPk, type Build } from "../pk";

async function engine(fluid: string, build: Build = "simd") {
  const pk = await loadPk(build);
  const handle = createHandler(pk, build === "simd" ? "simd128" : "baseline");
  let id = 0;
  const batch: Batch = (outputs, name1, values1, name2, values2) => {
    const r = handle({ id: ++id, op: "batch", fluid, outputs, name1, values1, name2, values2 } as Request);
    return r.ok && r.op === "batch"
      ? Promise.resolve(r.value)
      : Promise.reject(new Error(r.ok ? "not a batch" : r.error));
  };
  const constants = handle({ id: ++id, op: "constants", fluid, keys: [...LIMIT_KEYS] });
  const l = constants.ok && constants.op === "constants" ? limits(constants.value) : null;
  if (!l) throw new Error(`no limits for ${fluid}`);
  const registry = new pk.Registry();
  const props = (key: string, v: Vertex) =>
    registry.PropsSI(key, v.inputs[0], v.inputs[1], v.inputs[2], v.inputs[3], fluid);
  const phase = (v: Vertex) => {
    const state = registry.fluid(fluid).flash(v.inputs[0], v.inputs[1], v.inputs[2], v.inputs[3]);
    const p = state.phase;
    state.free();
    return p;
  };
  return { batch, limits: l, props, phase };
}

function vertices(data: DiagramData): Vertex[] {
  const dome = [...data.dome.liquid, ...data.dome.vapour, ...(data.dome.critical ? [data.dome.critical] : [])];
  return [...dome, ...data.lines.flatMap((l) => l.segments.flatMap((s) => s.vertices))];
}

// These check thousands of vertices against the package one by one, so they get a minute each.
const HEAVY = { timeout: 60_000 };

const cases: [string, string][] = [
  ["Water", "ph"],
  ["Water", "ts"],
  ["Water", "hs"],
  ["n-Propane", "ph"],
  ["n-Propane", "trho"],
  ["CarbonDioxide", "pt"],
];

describe.each(cases)("%s on %s", (fluid, id) => {
  test("every vertex is the package's value at its inputs", HEAVY, async () => {
    const e = await engine(fluid);
    const d = diagram(id) as Diagram;
    const data = await sampleDiagram(d, e.limits, e.batch);
    const all = vertices(data);
    expect(all.length).toBeGreaterThan(500);
    for (const v of all) {
      expect(Object.is(e.props(d.x.key, v), v.x), `${d.x.key} at ${v.inputs}`).toBe(true);
      expect(Object.is(e.props(d.y.key, v), v.y), `${d.y.key} at ${v.inputs}`).toBe(true);
    }
  });

  test("one-phase segments keep off the dome, and two-phase ones end on its saturation states", HEAVY, async () => {
    const e = await engine(fluid);
    const d = diagram(id) as Diagram;
    const data = await sampleDiagram(d, e.limits, e.batch);
    let [crossings, junctions] = [0, 0];
    const saturated = (line: { held: string; value: number }, v: Vertex | undefined) => {
      expect(v?.inputs[0], `${line.held}=${line.value}`).toBe(line.held);
      expect(v?.inputs[1]).toBe(line.value);
      expect(v?.inputs[2]).toBe("Q");
      expect([0, 1]).toContain(v?.inputs[3]);
    };
    for (const line of data.lines) {
      for (const segment of line.segments) {
        const inner = segment.vertices.slice(1, -1);
        if (segment.twoPhase) {
          expect(
            inner.every((v) => e.phase(v) === "twophase"),
            `${line.held}=${line.value}`,
          ).toBe(true);
        } else {
          expect(
            inner.every((v) => e.phase(v) !== "twophase"),
            `${line.held}=${line.value}`,
          ).toBe(true);
        }
        if (segment.twoPhase && (line.held === "T" || line.held === "P")) {
          crossings += 1;
          for (const end of [segment.vertices[0], segment.vertices.at(-1)]) saturated(line, end);
        }
      }
      // Wherever a line passes from one side of the dome to the other, the vertex both sides share is saturated.
      line.segments.forEach((segment, i) => {
        const next = line.segments[i + 1];
        if (next && next.twoPhase !== segment.twoPhase && next.vertices[0] === segment.vertices.at(-1)) {
          saturated(line, next.vertices[0]);
          junctions += 1;
        }
      });
    }
    if (id !== "pt") expect(crossings).toBeGreaterThan(3);
    expect(junctions).toBeGreaterThanOrEqual(2 * crossings);
  });
});

describe("the dome", () => {
  test("runs up the liquid side to the critical point and down the vapour side", HEAVY, async () => {
    const e = await engine("Water");
    const data = await sampleDiagram(diagram("ts") as Diagram, e.limits, e.batch);
    const { liquid, vapour, critical } = data.dome;
    expect(liquid.length).toBeGreaterThan(100);
    expect(vapour.length).toBeGreaterThan(100);
    const t = (v: Vertex) => e.props("T", v);
    expect(liquid.every((v, i) => i === 0 || t(v) > t(liquid[i - 1] as Vertex))).toBe(true);
    expect(liquid.every((v) => v.inputs[2] === "Q" && v.inputs[3] === 0)).toBe(true);
    expect(vapour.every((v) => v.inputs[2] === "Q" && v.inputs[3] === 1)).toBe(true);
    expect(critical?.inputs).toEqual(["T", e.limits.Tc, "Dmass", e.limits.rhoc]);
    expect(1 - t(liquid.at(-1) as Vertex) / e.limits.Tc).toBeLessThan(1e-3);
    // It starts where it enters the plotted pressures: at the saturation temperature of the lowest one.
    const [low] = ranges(e.limits).P;
    expect(liquid[0]?.inputs[1]).toBeGreaterThan(e.limits.Ttriple);
    expect(e.props("P", liquid[0] as Vertex) / low).toBeCloseTo(1, 6);
  });
});

describe("gaps", () => {
  test("a failed cell breaks its line, never joins across it", HEAVY, async () => {
    const e = await engine("Water");
    let failed = 0;
    const broken: Batch = async (outputs, name1, values1, name2, values2) => {
      const answer: BatchValue = await e.batch(outputs, name1, values1, name2, values2);
      if (name1 === "T" && values1[0] === 400 && name2 === "P") {
        const status = new Uint8Array(answer.status);
        for (let k = 0; k < answer.outputs; k++) status[50 * answer.outputs + k] = 4;
        failed += 1;
        return { ...answer, status };
      }
      return answer;
    };
    const data = await sampleDiagram(diagram("ph") as Diagram, e.limits, broken);
    const line = data.lines.find((l) => l.held === "T" && l.value === 400);
    expect(failed).toBe(1);
    const sweep = logGrid(...ranges(e.limits).P, 160);
    const lost = sweep[50];
    expect(line?.segments.flatMap((s) => s.vertices).some((v) => v.inputs[3] === lost)).toBe(false);
    expect(line?.segments.filter((s) => !s.twoPhase).length).toBe(3);
  });
});

describe("ranges", () => {
  const water: Limits = {
    Tc: 647.096,
    pc: 22064000,
    rhoc: 322,
    Ttriple: 273.16,
    ptriple: 611.65,
    Tmin: 273.16,
    Tmax: 2000,
    pmax: 1e9,
  };

  test("limits need every constant", () => {
    expect(limits([647.096, 22064000, 322, 273.16, 611.65, 273.16, 2000, 1e9])).toEqual(water);
    expect(limits([647.096, null, 322, 273.16, 611.65, 273.16, 2000, 1e9])).toBeNull();
    expect(limits([1, 2])).toBeNull();
  });

  test("span the fluid's range up to 1.6 T_c and 10 p_c", () => {
    expect(ranges(water)).toEqual({ T: [273.16, 647.096 * 1.6], P: [2206.4, 220640000] });
    expect(ranges({ ...water, Tmax: 700, pmax: 1e8, ptriple: 1e4, Tmin: 300 }).T).toEqual([300, 700]);
    expect(ranges({ ...water, Tmax: 700, pmax: 1e8, ptriple: 1e4 }).P).toEqual([1e4, 1e8]);
  });

  test("grids run from end to end", () => {
    expect(linGrid(0, 10, 6)).toEqual([0, 2, 4, 6, 8, 10]);
    const log = logGrid(1, 1000, 4);
    expect(log.map((v) => Math.round(v))).toEqual([1, 10, 100, 1000]);
    const ts = domeTemperatures(273.16, 647.096, 120);
    expect([ts.length, ts[0]]).toEqual([120, 273.16]);
    expect(ts.every((t, i) => i === 0 || t > (ts[i - 1] as number))).toBe(true);
    expect(647.096 - (ts.at(-1) as number)).toBeLessThan(0.1);
    expect(ts.at(-1)).toBeLessThan(647.096);
    // PLAN.md M21.4: on exactly the lowest temperature, which T_c − (T_c − low) misses by a rounding for R12 and
    // n-Pentane (their T_min and triple point), where the package then refuses the dome's first state.
    for (const [low, tc] of [
      [116.099, 385.1199997681392],
      [143.47, 469.6999998711195],
    ] as const)
      expect(domeTemperatures(low, tc, 120)[0]).toBe(low);
  });

  test("isoline values strictly inside: nice ticks; 1-2-5 per decade for log quantities, decades past 12 lines; tenths of quality", () => {
    expect(values("T", [273.16, 1035])).toEqual([300, 400, 500, 600, 700, 800, 900, 1000]);
    expect(values("P", [2206.4, 2.2064e8])).toEqual([1e4, 1e5, 1e6, 1e7, 1e8]);
    expect(values("Dmass", [0.05, 1000])).toEqual([0.1, 0.2, 0.5, 1, 2, 5, 10, 20, 50, 100, 200, 500]);
    expect(values("Dmass", [0.0048, 999])).toEqual([0.01, 0.1, 1, 10, 100]);
    expect(values("Smass", [0, 9000])).toEqual([1000, 2000, 3000, 4000, 5000, 6000, 7000, 8000]);
    expect(values("Q", [0, 1])).toEqual([0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9]);
  });
});

describe("diagrams", () => {
  test("five diagrams, each with at most three coloured families beside quality, and a readout pair of its axes", () => {
    expect(DIAGRAMS.map((d) => d.id)).toEqual(["ph", "ts", "pt", "trho", "hs"]);
    for (const d of DIAGRAMS) {
      expect(d.families.filter((f) => f.held !== "Q").length, d.id).toBeLessThanOrEqual(3);
      expect(new Set(d.readout), d.id).toEqual(new Set([d.x.key, d.y.key]));
      expect(
        d.families.every((f) => f.held !== d.x.key && f.held !== d.y.key),
        d.id,
      ).toBe(true);
    }
    expect(diagram("nonsense")).toBeUndefined();
  });
});

describe("segments", () => {
  const at = (n: number): Vertex => ({ x: n, y: n, inputs: ["T", n, "P", n] });
  const one = (n: number) => ({ vertex: at(n), twoPhase: false });
  const two = (n: number) => ({ vertex: at(n), twoPhase: true });
  const edge = (n: number) => ({ edge: at(n) });
  const shape = (s: ReturnType<typeof segments>) => s.map((g) => [g.twoPhase, g.vertices.map((v) => v.x)]);

  test("a failed cell ends a segment, and a lone vertex draws nothing", () => {
    expect(shape(segments([one(1), one(2), null, one(3), null, one(4), one(5)]))).toEqual([
      [false, [1, 2]],
      [false, [4, 5]],
    ]);
  });

  test("a change of side splits a line, and an edge vertex belongs to both sides", () => {
    expect(shape(segments([one(1), one(2), two(3), two(4)]))).toEqual([
      [false, [1, 2]],
      [true, [3, 4]],
    ]);
    expect(shape(segments([one(1), edge(2), two(3), edge(4), one(5)]))).toEqual([
      [false, [1, 2]],
      [true, [2, 3, 4]],
      [false, [4, 5]],
    ]);
    expect(shape(segments([edge(1), edge(2)]))).toEqual([[false, [1, 2]]]);
  });
});

/** The batch, with the status of some of its cells changed. */
function tamper(
  batch: Batch,
  when: (outputs: string[], name1: string, values1: Float64Array, name2: string, values2: Float64Array) => boolean,
  change: (status: Uint8Array, outputs: number) => void,
): Batch {
  return async (outputs, name1, values1, name2, values2) => {
    const answer = await batch(outputs, name1, values1, name2, values2);
    if (!when(outputs, name1, values1, name2, values2)) return answer;
    const status = new Uint8Array(answer.status);
    change(status, answer.outputs);
    return { ...answer, status };
  };
}

const isotherm400 = (_o: string[], n1: string, v1: Float64Array, n2: string) =>
  n1 === "T" && v1[0] === 400 && n2 === "P";

describe("cells", HEAVY, () => {
  test("a flagged extrapolation is a value; a cell whose x or y failed is a gap", async () => {
    const e = await engine("Water");
    const p = logGrid(...ranges(e.limits).P, 160);
    const has = (data: DiagramData, sweep: number) =>
      data.lines.some(
        (l) =>
          l.held === "T" && l.value === 400 && l.segments.some((s) => s.vertices.some((v) => v.inputs[3] === sweep)),
      );
    const flagged = tamper(e.batch, isotherm400, (status, m) => status.fill(1, 40 * m, 41 * m));
    expect(has(await sampleDiagram(diagram("ph") as Diagram, e.limits, flagged), p[40] as number)).toBe(true);
    for (const k of [0, 1]) {
      const gone = tamper(e.batch, isotherm400, (status, m) => (status[40 * m + k] = 4));
      expect(has(await sampleDiagram(diagram("ph") as Diagram, e.limits, gone), p[40] as number), `output ${k}`).toBe(
        false,
      );
    }
  });

  test("a saturation the package only partly gives is no crossing; one without its sweep value is no edge", async () => {
    const e = await engine("Water");
    const partly = tamper(
      e.batch,
      (_o, n1, v1, n2) => n1 === "T" && v1[0] === 400 && n2 === "Q",
      (status, m) => (status[3 * m] = 4),
    );
    const data = await sampleDiagram(diagram("ph") as Diagram, e.limits, partly);
    const line = data.lines.find((l) => l.held === "T" && l.value === 400);
    expect(line?.segments.some((s) => s.twoPhase)).toBe(false);
    const noSweep = tamper(
      e.batch,
      (_o, n1, _v1, n2) => n1 === "Smass" && n2 === "Q",
      (status, m) => {
        status[2] = 4;
        status[m + 2] = 4;
      },
    );
    const s = await sampleDiagram(diagram("ph") as Diagram, e.limits, noSweep);
    const edges = s.lines
      .filter((l) => l.held === "Smass")
      .flatMap((l) => l.segments.flatMap((g) => g.vertices))
      .filter((v) => v.inputs[2] === "Q");
    expect(edges).toEqual([]);
  });

  test("a failed cell of the dome is left out of it, on either side", async () => {
    const e = await engine("Water");
    const dome = (o: string[], n1: string, _v1: Float64Array, n2: string) => o.length === 5 && n1 === "T" && n2 === "Q";
    const data = await sampleDiagram(
      diagram("ts") as Diagram,
      e.limits,
      tamper(e.batch, dome, (status, m) => (status[10 * m] = 4)),
    );
    for (const side of [data.dome.liquid, data.dome.vapour]) {
      expect(side.length).toBe(119);
      expect(side.every((v) => v !== null && Number.isFinite(v.x))).toBe(true);
    }
  });
});

describe("shapes", HEAVY, () => {
  test("an isotherm enters the dome from the vapour, an isobar from the liquid; quality lines are drawn", async () => {
    const e = await engine("Water");
    const junction = (data: DiagramData, held: string) =>
      data.lines
        .filter((l) => l.held === held)
        .flatMap((l) => {
          const i = l.segments.findIndex((s) => s.twoPhase);
          return i > 0 ? [l.segments[i]?.vertices[0]?.inputs[3]] : [];
        });
    const ph = await sampleDiagram(diagram("ph") as Diagram, e.limits, e.batch);
    expect(junction(ph, "T").length).toBeGreaterThan(2);
    expect(new Set(junction(ph, "T"))).toEqual(new Set([1]));
    const ts = await sampleDiagram(diagram("ts") as Diagram, e.limits, e.batch);
    expect(junction(ts, "P").length).toBeGreaterThan(2);
    expect(new Set(junction(ts, "P"))).toEqual(new Set([0]));
    const quality = ph.lines.filter((l) => l.held === "Q");
    expect(quality.map((l) => l.value)).toEqual([0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9]);
    expect(quality.every((l) => l.segments.length === 1 && (l.segments[0]?.vertices.length ?? 0) > 100)).toBe(true);
  });

  test("an axis of s, h or ρ spans the dome's and the corners' values", async () => {
    const e = await engine("Water");
    const ts = await sampleDiagram(diagram("ts") as Diagram, e.limits, e.batch);
    const xs = [...ts.dome.liquid, ...ts.dome.vapour].map((v) => v.x);
    expect(ts.x[0]).toBeLessThanOrEqual(Math.min(...xs));
    expect(ts.x[1]).toBeGreaterThanOrEqual(Math.max(...xs));
    expect(ts.x[1]).toBeLessThan(2e4);
    const trho = await sampleDiagram(diagram("trho") as Diagram, e.limits, e.batch);
    expect(trho.x[0]).toBeGreaterThan(0);
    expect(trho.x[1]).toBeLessThan(2000);
    expect(trho.y).toEqual(ranges(e.limits).T);
  });
});

describe("extents", () => {
  test("take each key's values where the package gave them, whatever became of x and y", () => {
    const row = (cells: number[], ok: boolean[]): Row => ({ cells, ok, inputs: ["T", 1, "P", 1] });
    const found = [
      row([0, 0, 10, 20, 30], [false, false, true, true, true]),
      row([0, 0, 1, 2, 3], [true, true, true, true, true]),
      row([0, 0, -5, -5, -5], [true, true, false, false, false]),
    ];
    expect(extents(found, 2)).toEqual({ Hmass: [1, 10], Smass: [2, 20], Dmass: [3, 30] });
  });
});

describe("grid ends", () => {
  test("land on b exactly, wherever a computed end would round off", () => {
    expect(linGrid(0.2, 0.9, 8).at(-1)).toBe(0.9);
    expect(linGrid(1, 3, 3)).toEqual([1, 2, 3]);
    expect(logGrid(0.1, 27.500000000000004, 3).at(-1)).toBe(27.500000000000004);
    expect(logGrid(2, 8, 3)).toEqual([2, 4, 8]);
    expect(limits([1, 2, 3, 4, 5, 6, 7, Number.NaN])).toBeNull();
  });
});

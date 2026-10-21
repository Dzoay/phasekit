// The diagrams' geometry as the page draws it (PLAN.md M21.2, M21.6) under Node on the real package: it adds no
// numerics, so every vertex is the package's value at that vertex's inputs (`Exact`); a one-phase segment lies on one
// side of the dome, a two-phase one ends on the saturation states at its line's value; the dome runs up to the critical
// point; and the page's table of planes is the package's.
import { describe, expect, test } from "vitest";
import type { Vertex } from "../../src/diagram/data";
import { DIAGRAMS, diagram } from "../../src/diagram/planes";
import { ranges } from "../../src/lib/ranges";
import { drawn, inputsOf, vertices } from "./drawn";

// These check thousands of vertices against the package one by one, so they get a minute each.
const HEAVY = { timeout: 60_000 };

// Each with the fewest vertices it draws: p–T has the fewest lines.
const cases: [string, string, number][] = [
  ["Water", "ph", 500],
  ["Water", "ts", 500],
  ["Water", "hs", 500],
  ["n-Propane", "ph", 500],
  ["n-Propane", "trho", 500],
  ["CarbonDioxide", "pt", 400],
];

describe.each(cases)("%s on %s", (fluid, id, fewest) => {
  test("every vertex is the package's value at its inputs", HEAVY, async () => {
    const e = await drawn(fluid);
    const d = diagram(id);
    const all = vertices(e.diagram(id));
    expect(all.length).toBeGreaterThan(fewest);
    for (const v of all) {
      expect(Object.is(e.props(d?.x.key as string, v), v.x), `${d?.x.key} at ${v.inputs}`).toBe(true);
      expect(Object.is(e.props(d?.y.key as string, v), v.y), `${d?.y.key} at ${v.inputs}`).toBe(true);
    }
  });

  test("one-phase segments keep off the dome, and two-phase ones end on its saturation states", HEAVY, async () => {
    const e = await drawn(fluid);
    const data = e.diagram(id);
    const phase = (v: Vertex) => e.phaseAt(...v.inputs);
    let [crossings, junctions] = [0, 0];
    const saturated = (line: { held: string; value: number }, v: Vertex | undefined) => {
      const inputs = inputsOf(v as Vertex);
      expect(inputs[line.held], `${line.held}=${line.value}`).toBe(line.value);
      expect([0, 1]).toContain(inputs.Q);
    };
    for (const line of data.lines) {
      for (const segment of line.segments) {
        const inner = segment.vertices.slice(1, -1);
        const twoPhase = inner.filter((v) => phase(v) === "twophase").length;
        expect(twoPhase, `${line.held}=${line.value}`).toBe(segment.twoPhase ? inner.length : 0);
        if (segment.twoPhase && (line.held === "T" || line.held === "P")) {
          crossings += 1;
          for (const end of [segment.vertices[0], segment.vertices.at(-1)]) saturated(line, end);
        }
      }
      // Wherever a line passes from one side of the dome to the other, the vertex both sides share is saturated.
      line.segments.forEach((segment, i) => {
        const next = line.segments[i + 1];
        const [last, first] = [segment.vertices.at(-1), next?.vertices[0]];
        if (next && next.twoPhase !== segment.twoPhase && first?.x === last?.x && first?.y === last?.y) {
          saturated(line, first);
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
    const e = await drawn("Water");
    const { liquid, vapour, critical } = e.diagram("ts").dome;
    expect(liquid.length).toBeGreaterThan(100);
    expect(vapour.length).toBeGreaterThan(100);
    const t = (v: Vertex) => inputsOf(v).T as number;
    expect(liquid.every((v, i) => i === 0 || t(v) > t(liquid[i - 1] as Vertex))).toBe(true);
    expect(liquid.every((v) => inputsOf(v).Q === 0)).toBe(true);
    expect(vapour.every((v) => inputsOf(v).Q === 1)).toBe(true);
    expect(critical?.inputs).toEqual(["Dmass", e.limits.rhoc, "T", e.limits.Tc]);
    expect(1 - t(liquid.at(-1) as Vertex) / e.limits.Tc).toBeLessThan(1e-3);
    // It starts where it enters the plotted pressures: at the saturation temperature of the lowest one.
    const [low] = ranges(e.limits).P;
    expect(t(liquid[0] as Vertex)).toBeGreaterThan(e.limits.Ttriple);
    expect((e.props("P", liquid[0] as Vertex) as number) / low).toBeCloseTo(1, 6);
  });
});

describe("shapes", HEAVY, () => {
  test("an isotherm enters the dome from the vapour, an isobar from the liquid; quality lines are drawn", async () => {
    const e = await drawn("Water");
    const junction = (held: string, data: ReturnType<typeof e.diagram>) =>
      data.lines
        .filter((l) => l.held === held)
        .flatMap((l) => {
          const i = l.segments.findIndex((s) => s.twoPhase);
          return i > 0 ? [inputsOf(l.segments[i]?.vertices[0] as Vertex).Q] : [];
        });
    const ph = e.diagram("ph");
    expect(junction("T", ph).length).toBeGreaterThan(2);
    expect(new Set(junction("T", ph))).toEqual(new Set([1]));
    const ts = e.diagram("ts");
    expect(junction("P", ts).length).toBeGreaterThan(2);
    expect(new Set(junction("P", ts))).toEqual(new Set([0]));
    const quality = ph.lines.filter((l) => l.held === "Q");
    expect(quality.map((l) => l.value)).toEqual([0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9]);
    expect(quality.every((l) => l.segments.length === 1 && (l.segments[0]?.vertices.length ?? 0) > 100)).toBe(true);
  });

  test("an axis of s, h or ρ spans the dome's values; T and p span the plotted ranges", async () => {
    const e = await drawn("Water");
    const ts = e.diagram("ts");
    const xs = [...ts.dome.liquid, ...ts.dome.vapour].map((v) => v.x);
    expect(ts.x[0]).toBeLessThanOrEqual(Math.min(...xs));
    expect(ts.x[1]).toBeGreaterThanOrEqual(Math.max(...xs));
    expect(ts.x[1]).toBeLessThan(2e4);
    const trho = e.diagram("trho");
    expect(trho.x[0]).toBeGreaterThan(0);
    expect(trho.x[1]).toBeLessThan(2000);
    expect(trho.y).toEqual(ranges(e.limits).T);
    expect(e.diagram("pt").y).toEqual(ranges(e.limits).P);
  });
});

describe("the planes", () => {
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

  test("are the package's: its planes' axes and families, in its order", async () => {
    const { pk } = await drawn("Water");
    const planes = pk.planes().map((p) => {
      const plane = {
        id: p.id,
        x: { key: p.xKey, log: p.xLog },
        y: { key: p.yKey, log: p.yLog },
        families: p.held.map((held, i) => ({ held, sweep: p.sweep[i] })),
      };
      p.free();
      return plane;
    });
    expect(planes).toEqual(DIAGRAMS.map(({ id, x, y, families }) => ({ id, x, y, families })));
  });
});

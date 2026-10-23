// The diagrams' overlays (PLAN.md M26.6): src/diagram/overlays.ts reads the package's curves and maps, and
// src/diagram/model.ts draws them, on hand-made answers whose every number the test can follow.
import { scaleLinear } from "d3-scale";
import { describe, expect, test } from "vitest";
import type { CurveRunsValue, CurvesValue, MapValue } from "../../src/engine/protocol";
import { buildModel, mapLabel } from "../../src/diagram/model";
import {
  CURVES,
  CURVE_PLANES,
  MAP_OUTPUTS,
  bandColour,
  channels,
  readCurves,
  readMap,
} from "../../src/diagram/overlays";
import { diagram, type Diagram } from "../../src/diagram/planes";
import type { DiagramData } from "../../src/diagram/data";

// Pair 0 is (D, T), pair 1 (Q, T), pair 2 (P, T).
const PAIR_KEYS = ["Dmass", "T", "Q", "T", "P", "T"];

function runs(states: [number, number, number, number, number][][]): CurveRunsValue {
  const all = states.flat();
  const starts = [0];
  for (const r of states) starts.push((starts.at(-1) as number) + r.length);
  return {
    starts: new Uint32Array(starts),
    T: new Float64Array(all.map((s) => s[0])),
    rho: new Float64Array(all.map((s) => s[1])),
    p: new Float64Array(all.map((s) => s[2])),
    Z: new Float64Array(all.map((s) => s[3])),
    inputs: new Float64Array(all.flatMap((s) => [s[1], s[0]])),
    pairs: new Uint8Array(all.map((s) => s[4])),
  };
}

const CURVES_VALUE: CurvesValue = {
  names: ["Ideal", "Boyle"],
  zeroDensity: new Float64Array([400, NaN]),
  curves: new Uint32Array([0, 2, 3]),
  lines: runs([
    [
      [300, 1, 1e5, 1, 0],
      [310, 2, 2e5, 1, 0],
    ],
    [[320, 3, 3e5, 1, 0]],
    [
      [330, 4, 4e5, 1.1, 0],
      [340, 5, 5e5, 1.2, 0],
    ],
  ]),
  isotherms: new Float64Array([100, 200]),
  // 100 K: two vapour states, the saturated vapour and liquid (QT), then the liquid; 200 K: two states.
  chart: runs([
    [
      [100, 1, 1e4, 0.99, 2],
      [100, 2, 2e4, 0.95, 1],
      [100, 800, 2e4, 0.004, 1],
      [100, 810, 1e6, 0.1, 2],
    ],
    [
      [200, 1, 1e4, 1, 2],
      [200, 2, 1e6, 0.9, 2],
    ],
  ]),
  pairKeys: PAIR_KEYS,
};

describe("the curves' reader", () => {
  test("gives each curve its lines, and its zero-density end or none", () => {
    const c = readCurves(CURVES_VALUE);
    expect(c.curves.map((x) => [x.name, x.lines.length, x.zeroDensity])).toEqual([
      ["Ideal", 2, 400],
      ["Boyle", 1, null],
    ]);
    expect(c.curves[0]?.lines[0]).toEqual([
      { T: 300, rho: 1, p: 1e5, Z: 1 },
      { T: 310, rho: 2, p: 2e5, Z: 1 },
    ]);
    expect(c.curves[1]?.lines[0]?.map((s) => s.T)).toEqual([330, 340]);
  });

  test("cuts an isotherm of the chart between its saturated vapour and liquid, and no other", () => {
    const c = readCurves(CURVES_VALUE);
    expect(c.chart.map((i) => [i.T, i.runs.map((r) => r.map((s) => s.rho))])).toEqual([
      [
        100,
        [
          [1, 2],
          [800, 810],
        ],
      ],
      [200, [[1, 2]]],
    ]);
  });

  test("names each curve and draws it on p–T and T–ρ only", () => {
    expect(Object.keys(CURVES)).toEqual(["Ideal", "Boyle", "JouleThomsonInversion", "JouleInversion"]);
    expect(new Set(Object.values(CURVES).map((c) => c.colour)).size).toBe(4);
    expect(CURVE_PLANES).toEqual({ pt: ["T", "p"], trho: ["rho", "T"] });
  });
});

const MAP_VALUE: MapValue = {
  plane: "pt",
  key: "Dmass",
  values: new Float64Array([1, 900]),
  x: new Float64Array([200, 400]),
  y: new Float64Array([1e4, 1e7]),
  levels: new Float64Array([10, 100]),
  contours: new Uint32Array([0, 1, 3]),
  lines: {
    starts: new Uint32Array([0, 2, 4, 6]),
    vertices: {
      xy: new Float64Array([250, 1e5, 260, 2e5, 270, 1e6, 280, 2e6, 290, 3e6, 300, 4e6]),
      inputs: new Float64Array([1e5, 250, 2e5, 260, 1e6, 270, 2e6, 280, 3e6, 290, 4e6, 300]),
      pairs: new Uint8Array([2, 2, 2, 2, 2, 2]),
    },
  },
  low: new Float64Array([NaN, 10, 100]),
  high: new Float64Array([10, 100, NaN]),
  bands: new Uint32Array([0, 1, 1, 2]),
  polygons: {
    starts: new Uint32Array([0, 3, 6]),
    vertices: {
      xy: new Float64Array([200, 1e4, 300, 1e4, 200, 1e5, 300, 1e6, 400, 1e6, 400, 1e7]),
      inputs: new Float64Array([1e4, 200, 1e4, 300, 1e5, 200, 1e6, 300, 1e6, 400, 1e7, 400]),
      pairs: new Uint8Array([2, 2, 2, 2, 2, 2]),
    },
  },
  pairKeys: PAIR_KEYS,
};

describe("the map's reader", () => {
  test("gives each level its lines and each band its bounds, null where unbounded, and its polygons", () => {
    const m = readMap(MAP_VALUE);
    expect([m.key, m.values, m.levels]).toEqual(["Dmass", [1, 900], [10, 100]]);
    expect(m.contours.map((c) => c.length)).toEqual([1, 2]);
    expect(m.contours[1]?.[1]?.[0]).toEqual({ x: 290, y: 3e6, inputs: ["P", 3e6, "T", 290] });
    expect(m.bands.map((b) => [b.low, b.high, b.polygons.length])).toEqual([
      [null, 10, 1],
      [10, 100, 0],
      [100, null, 1],
    ]);
    expect(m.bands[2]?.polygons[0]?.map((v) => v.x)).toEqual([300, 400, 400]);
  });

  test("offers the planes' quantities and the outputs a map shows best", () => {
    expect(MAP_OUTPUTS).toContain("speed_of_sound");
    expect(MAP_OUTPUTS.slice(0, 5)).toEqual(["T", "P", "Dmass", "Hmass", "Smass"]);
  });
});

describe("a map's colours", () => {
  test("reads rgb() and #rrggbb colours, and nothing else", () => {
    expect(channels("rgb(1, 2, 3)")).toEqual([1, 2, 3]);
    expect(channels("rgba(10 20 30 / 50%)")).toEqual([10, 20, 30]);
    expect(channels("#0a14FF")).toEqual([10, 20, 255]);
    expect(channels("currentColor")).toBeNull();
    expect(channels("#0a14f")).toBeNull();
  });

  test("ramps its bands evenly from the low colour to the high one", () => {
    const [low, high] = ["rgb(0, 100, 200)", "rgb(100, 200, 0)"];
    expect([0, 1, 2].map((j) => bandColour(j, 3, low, high))).toEqual([
      "rgb(0, 100, 200)",
      "rgb(50, 150, 100)",
      "rgb(100, 200, 0)",
    ]);
    expect(bandColour(0, 1, low, high)).toBe(low);
    expect(bandColour(1, 3, "currentColor", high)).toBe("currentColor");
    expect(bandColour(1, 3, low, "currentColor")).toBe(low);
  });

  test("labels a level in its output's shown unit", () => {
    expect(mapLabel("T", 300, "si")).toBe("300 K");
    expect(mapLabel("T", 300, "engineering")).toBe("26.85 °C");
    expect(mapLabel("Z", 0.5, "si")).toBe("0.5");
    expect(mapLabel("nothing", 2, "si")).toBe("2");
  });
});

describe("the overlays drawn", () => {
  const empty: DiagramData = {
    dome: { liquid: [], vapour: [], critical: null },
    lines: [],
    x: [200, 400],
    y: [1e4, 1e7],
  } as unknown as DiagramData;
  const plot = { left: 0, top: 0, right: 1000, bottom: 1000 };
  const options = (d: Diagram) => ({
    x: scaleLinear().domain([200, 400]).range([0, 1000]),
    y: scaleLinear().domain([0, 1e7]).range([1000, 0]),
    plot,
    system: "si" as const,
    hidden: new Set<never>(),
    colour: (token: string) =>
      token === "--map-low" ? "rgb(0, 0, 0)" : token === "--map-high" ? "rgb(90, 90, 90)" : token,
    d,
  });

  test("draws the curves on p–T, each in its own style, and not on p–h", () => {
    const curves = readCurves(CURVES_VALUE);
    const pt = diagram("pt") as Diagram;
    const m = buildModel(pt, empty, { ...options(pt), curves });
    const drawn = m.strokes.slice(1);
    expect(drawn.map((s) => [s.colour, s.dash, s.width, s.points.length])).toEqual([
      ["--curve-1", [], 2, 2],
      ["--curve-1", [], 2, 1],
      ["--curve-2", [7, 3], 2, 2],
    ]);
    expect(drawn[0]?.points[0]).toEqual([500, 990]);
    expect(m.labels.map((l) => l.text)).toEqual(["Ideal curve (Z = 1)", "Boyle curve"]);
    const ph = diagram("ph") as Diagram;
    expect(buildModel(ph, empty, { ...options(ph), curves }).strokes).toHaveLength(1);
  });

  test("fills a map's bands on its ramp under its contour lines, each labelled with its level", () => {
    const map = readMap(MAP_VALUE);
    const pt = diagram("pt") as Diagram;
    const m = buildModel(pt, empty, { ...options(pt), map });
    expect(m.bands.map((b) => [b.colour, b.points.length])).toEqual([
      ["rgb(0, 0, 0)", 3],
      ["rgb(90, 90, 90)", 3],
    ]);
    expect(m.halo).toBe("--surface");
    expect(m.strokes.slice(1).map((s) => [s.colour, s.width, s.points.length])).toEqual([
      ["--fg-muted", 1, 2],
      ["--fg-muted", 1, 2],
      ["--fg-muted", 1, 2],
    ]);
    expect(m.labels.map((l) => l.text)).toEqual(["10 kg/m³", "100 kg/m³"]);
    expect(buildModel(pt, empty, options(pt)).bands).toEqual([]);
  });
});

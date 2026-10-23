// The curves page's charts (PLAN.md M26.6): src/curves/chart.ts scales the package's curves and chart into paths, on
// hand-made states whose every number the test can follow.
import { describe, expect, test } from "vitest";
import { apart, curvesChart, extent, frame, rampToken, zChart, type Path } from "../../src/curves/chart";
import type { ChartIsotherm, Curve } from "../../src/diagram/overlays";

const colour = (token: string) => token;
const state = (T: number, p: number, Z = 1) => ({ T, rho: 1, p, Z });
const vertex = (x: number, y: number) => ({ x, y, inputs: ["Q", 0, "T", x] as const });

describe("the charts' frame and scales", () => {
  test("leaves its margins", () => {
    expect(frame(800, 400)).toEqual({ width: 800, height: 400, left: 64, top: 12, right: 716, bottom: 356 });
  });

  test("spans the positive finite values, and nothing where there are none", () => {
    expect(extent([3, -1, 0, NaN, Infinity, 1, 10])).toEqual([1, 10]);
    expect(extent([0, -2, NaN])).toBeNull();
    expect(extent([])).toBeNull();
  });

  test("spreads the sequential ramp's six steps over the isotherms, coldest first", () => {
    expect([0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10].map((j) => rampToken(j, 11))).toEqual([
      "--seq-1",
      "--seq-2",
      "--seq-2",
      "--seq-3",
      "--seq-3",
      "--seq-4",
      "--seq-4",
      "--seq-5",
      "--seq-5",
      "--seq-6",
      "--seq-6",
    ]);
    expect(rampToken(0, 1)).toBe("--seq-1");
  });
});

describe("the curves chart", () => {
  const curves: Curve[] = [
    { name: "Ideal", lines: [[state(100, 1e3), state(1000, 1e7)]], zeroDensity: null },
    { name: "Boyle", lines: [[state(200, 1e4)], [state(300, 1e5)]], zeroDensity: 900 },
    { name: "Unknown", lines: [[state(500, 1e5)]], zeroDensity: null },
  ];
  const f = frame(764, 400);

  test("draws log p against log T, the saturation line first, each curve's line in its style", () => {
    const c = curvesChart(curves, [vertex(100, 1e3), vertex(200, 1e5)], vertex(200, 1e5), f, colour);
    expect(c.paths.map((p) => [p.colour, p.dash, p.width])).toEqual([
      ["--fg", "", 2],
      ["--curve-1", "", 2],
      ["--curve-2", "7 3", 2],
      ["--curve-2", "7 3", 2],
    ]);
    expect(c.paths[1]?.d).toBe("M64,356L680,12");
    expect(c.dots).toHaveLength(1);
    expect(c.dots[0]?.x).toBeCloseTo(64 + 616 * Math.log10(2), 9);
    expect(c.dots[0]?.y).toBeCloseTo(356 - 344 * 0.5, 9);
    expect(c.xTicks.map((t) => [t.at, t.text])).toEqual([
      [64, "100"],
      [680, "1000"],
    ]);
    expect(c.yTicks.map((t) => t.text)).toEqual(["1000", "10⁴", "10⁵", "10⁶", "10⁷"]);
  });

  test("marks no critical point where there is none, and scales something where there is nothing", () => {
    const c = curvesChart([], [], null, f, colour);
    expect([c.dots, c.paths.length]).toEqual([[], 1]);
  });
});

describe("the compressibility chart", () => {
  const chart: ChartIsotherm[] = [
    {
      T: 100,
      runs: [
        [state(0, 1e3, 0.9), state(0, 1e4, 0.8)],
        [state(0, 1e4, 0.1), state(0, 1e5, 0.2)],
      ],
    },
    { T: 200, runs: [[state(0, 1e3, 1), state(0, 1e5, 1.3)]] },
  ];

  test("draws Z against log p, an isotherm a ramp's step, labelled at its last run's end", () => {
    const c = zChart(chart, frame(764, 400), colour);
    expect(c.paths.map((p) => [p.colour, p.width, p.label?.text ?? null])).toEqual([
      ["--seq-1", 1.5, null],
      ["--seq-1", 1.5, "100 K"],
      ["--seq-6", 1.5, "200 K"],
    ]);
    expect(c.paths[1]?.label).toMatchObject({ x: 680 });
    expect(c.yTicks[0]).toEqual({ at: 356, text: "0" });
    expect(c.xTicks.map((t) => t.text)).toEqual(["1000", "10⁴", "10⁵"]);
  });

  test("spans Z from 0 to 1.1 at least", () => {
    const c = zChart([{ T: 1, runs: [[state(0, 1e3, 0.5), state(0, 1e5, 0.6)]] }], frame(764, 400), colour);
    expect([c.yTicks[0]?.text, c.yTicks.at(-1)?.text]).toEqual(["0", "1"]);
    expect(zChart([], frame(764, 400), colour).paths).toEqual([]);
  });
});

describe("the labels kept apart", () => {
  const at = (y: number | null): Path => ({
    d: "",
    colour: "",
    dash: "",
    width: 1,
    label: y === null ? null : { x: 0, y, text: String(y) },
  });

  test("drops a label nearer than 12 px to one kept, and keeps its line", () => {
    const got = apart([at(100), at(null), at(111.9), at(112), at(88.5), at(87)]);
    expect(got.map((p) => p.label?.y ?? null)).toEqual([100, null, null, 112, null, 87]);
    expect(got).toHaveLength(6);
  });
});

// The diagrams' snap grid (PLAN.md M21.8): the values an axis snaps to at each density, the nearest of them, a step
// along them, and how the grid reads.
import { describe, expect, test } from "vitest";
import { GRID_LEVELS, describeGrid, gridValues, snap, step } from "../../src/diagram/grid";

describe("grid values", () => {
  test("on a linear axis are d3's nice steps, finer with each level", () => {
    expect(GRID_LEVELS).toBe(5);
    const steps = [1, 2, 3, 4, 5].map((level) => {
      const v = gridValues([0, 4000], false, level);
      return [v[0], (v[1] as number) - (v[0] as number), v.at(-1)];
    });
    expect(steps).toEqual([
      [0, 1000, 4000],
      [0, 500, 4000],
      [0, 200, 4000],
      [0, 100, 4000],
      [0, 50, 4000],
    ]);
    expect(gridValues([0.1, 0.35], false, 1)).toEqual([0.1, 0.15, 0.2, 0.25, 0.3, 0.35]);
  });

  test("on a log axis are decades, 1-2-5, 1…9, halves and tenths per decade, as round decimals", () => {
    expect(gridValues([1e3, 1e6], true, 1)).toEqual([1e3, 1e4, 1e5, 1e6]);
    expect(gridValues([1e3, 1e4], true, 2)).toEqual([1000, 2000, 5000, 10000]);
    expect(gridValues([1, 10], true, 3)).toEqual([1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
    expect(gridValues([1, 3], true, 4)).toEqual([1, 1.5, 2, 2.5, 3]);
    expect(gridValues([0.01, 0.012], true, 5)).toEqual([0.01, 0.011, 0.012]);
    expect(gridValues([1e-7, 1.3e-7], true, 5)).toEqual([1e-7, 1.1e-7, 1.2e-7, 1.3e-7]);
    expect(gridValues([150, 450], true, 1)).toEqual([]);
  });
});

describe("snapping", () => {
  const linear = [1000, 1200, 1400];
  const log = [1e4, 2e4, 5e4];

  test("takes the nearest value, in log space on a log axis, the lower one on a tie", () => {
    expect(snap(1234, linear, false)).toBe(1200);
    expect(snap(1300, linear, false)).toBe(1200);
    expect(snap(1301, linear, false)).toBe(1400);
    expect(snap(-5, linear, false)).toBe(1000);
    expect(snap(9e9, linear, false)).toBe(1400);
    expect(snap(3.2e4, log, false)).toBe(2e4);
    expect(snap(3.2e4, log, true)).toBe(5e4);
    expect(snap(7, [], false)).toBe(7);
  });

  test("steps one value along from the one snapping finds, and stops at the ends", () => {
    expect(step(1200, linear, false, 1)).toBe(1400);
    expect(step(1200, linear, false, -1)).toBe(1000);
    expect(step(1400, linear, false, 1)).toBe(1400);
    expect(step(1390, linear, false, 1)).toBe(1400);
    expect(step(1000, linear, false, -1)).toBe(1000);
    expect(step(1290, linear, false, 1)).toBe(1400);
    // 3.2e4 is nearest 2e4 in value and 5e4 in log space.
    expect(step(3.2e4, log, false, -1)).toBe(1e4);
    expect(step(3.2e4, log, true, -1)).toBe(2e4);
    expect(step(7, [], false, 1)).toBe(7);
  });
});

describe("the grid's description", () => {
  test("names its step on a linear axis, its mantissas on a log one", () => {
    expect(describeGrid([0, 4000], false, 5)).toBe("every 50");
    expect(describeGrid([0, 1], false, 2)).toBe("every 0.1");
    expect(describeGrid([1000, 2000], false, 1)).toBe("every 200");
    expect([1, 2, 3, 4, 5].map((level) => describeGrid([1, 1e6], true, level))).toEqual([
      "decades",
      "1, 2, 5 per decade",
      "1 to 9 per decade",
      "halves per decade",
      "tenths per decade",
    ]);
  });
});

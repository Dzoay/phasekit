// A fluid's plotted ranges and the grids over them (PLAN.md M21.2), which the sweeps and the benchmark sample on.
import { describe, expect, test } from "vitest";
import { domeTemperatures, limits, linGrid, logGrid, ranges, type Limits } from "../../src/lib/ranges";

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
    expect(limits([1, 2, 3, 4, 5, 6, 7, Number.NaN])).toBeNull();
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
    // n-Pentane (their T_min and triple point), where the package then refuses the first saturated state.
    for (const [low, tc] of [
      [116.099, 385.1199997681392],
      [143.47, 469.6999998711195],
    ] as const)
      expect(domeTemperatures(low, tc, 120)[0]).toBe(low);
  });

  test("land on b exactly, wherever a computed end would round off", () => {
    expect(linGrid(0.2, 0.9, 8).at(-1)).toBe(0.9);
    expect(linGrid(1, 3, 3)).toEqual([1, 2, 3]);
    expect(logGrid(0.1, 27.500000000000004, 3).at(-1)).toBe(27.500000000000004);
    expect(logGrid(2, 8, 3)).toEqual([2, 4, 8]);
  });
});

// Reading the package's geometry (PLAN.md M21.6): the engine's typed arrays as the vertices, lines, corners and regions
// the page draws, every number where the package put it.
import { describe, expect, test } from "vitest";
import { readDiagram, readRegions } from "../../src/diagram/data";
import type { DiagramValue, RegionsValue, VerticesValue } from "../../src/engine/protocol";

/** Vertices `[x, y, input1, input2, pair]` as the package lists them. */
const list = (...vs: [number, number, number, number, number][]): VerticesValue => ({
  xy: Float64Array.from(vs.flatMap(([x, y]) => [x, y])),
  inputs: Float64Array.from(vs.flatMap(([, , a, b]) => [a, b])),
  pairs: Uint8Array.from(vs.map((v) => v[4])),
});

const PAIR_KEYS = ["Q", "T", "P", "Q", "Dmass", "T"];

function value(over: Partial<DiagramValue> = {}): DiagramValue {
  return {
    x: Float64Array.from([1, 2]),
    y: Float64Array.from([3, 4]),
    liquid: list([10, 11, 0, 300, 0], [12, 13, 0, 310, 0]),
    vapour: list([14, 15, 1, 300, 0]),
    critical: list([16, 17, 322, 647, 2]),
    lines: {
      held: ["T", "P", "Smass"],
      values: Float64Array.from([400, 1e5, 7000]),
      // The isotherm in two segments, the isobar in none, the isentrope in one.
      segments: Uint32Array.from([0, 2, 2, 3]),
      twoPhase: Uint8Array.from([0, 1, 0]),
      starts: Uint32Array.from([0, 2, 5, 7]),
      vertices: list(
        [20, 21, 1e5, 1, 1],
        [22, 23, 2e5, 1, 1],
        [24, 25, 1, 400, 0],
        [26, 27, 0.5, 400, 0],
        [28, 29, 0, 400, 0],
        [30, 31, 900, 400, 2],
        [32, 33, 950, 400, 2],
      ),
    },
    pairKeys: PAIR_KEYS,
    mixture: null,
    ...over,
  };
}

describe("a diagram, read", () => {
  test("has the package's dome, critical point, axes and lines, each vertex with its pair's keys", () => {
    expect(readDiagram(value())).toEqual({
      dome: {
        liquid: [
          { x: 10, y: 11, inputs: ["Q", 0, "T", 300] },
          { x: 12, y: 13, inputs: ["Q", 0, "T", 310] },
        ],
        vapour: [{ x: 14, y: 15, inputs: ["Q", 1, "T", 300] }],
        critical: { x: 16, y: 17, inputs: ["Dmass", 322, "T", 647] },
      },
      lines: [
        {
          held: "T",
          value: 400,
          segments: [
            {
              twoPhase: false,
              vertices: [
                { x: 20, y: 21, inputs: ["P", 1e5, "Q", 1] },
                { x: 22, y: 23, inputs: ["P", 2e5, "Q", 1] },
              ],
            },
            {
              twoPhase: true,
              vertices: [
                { x: 24, y: 25, inputs: ["Q", 1, "T", 400] },
                { x: 26, y: 27, inputs: ["Q", 0.5, "T", 400] },
                { x: 28, y: 29, inputs: ["Q", 0, "T", 400] },
              ],
            },
          ],
        },
        { held: "P", value: 1e5, segments: [] },
        {
          held: "Smass",
          value: 7000,
          segments: [
            {
              twoPhase: false,
              vertices: [
                { x: 30, y: 31, inputs: ["Dmass", 900, "T", 400] },
                { x: 32, y: 33, inputs: ["Dmass", 950, "T", 400] },
              ],
            },
          ],
        },
      ],
      x: [1, 2],
      y: [3, 4],
    });
  });

  test("without a critical point has none, and a mixture's carries its envelope's end", () => {
    const mixture = { components: ["A", "B"], feed: [0.25, 0.75], end: "bound", reason: "below T_min" };
    const read = readDiagram(value({ critical: list(), mixture }));
    expect(read.dome.critical).toBeNull();
    expect(read.envelope).toBe(mixture);
    expect("envelope" in readDiagram(value())).toBe(false);
  });
});

/** Corners `[T, P, h, s, ρ]` in runs, as the package lists them. */
const runs = (...rs: number[][][]) => ({
  starts: Uint32Array.from([0, ...rs.map((_, i) => rs.slice(0, i + 1).flat().length)]),
  corners: Float64Array.from(rs.flat(2)),
});

const corner = ([T, P, Hmass, Smass, Dmass]: number[]) => ({ T, P, Hmass, Smass, Dmass });

describe("the phase regions, read", () => {
  const regions: RegionsValue = {
    phases: ["gas", "liquid"],
    outlines: runs(
      [
        [1, 2, 3, 4, 5],
        [6, 7, 8, 9, 10],
      ],
      [[11, 12, 13, 14, 15]],
    ),
    isotherm: runs([[20, 21, 22, 23, 24]], [[25, 26, 27, 28, 29]]),
    isobar: runs(),
    critical: Float64Array.from([30, 31, 32, 33, 34]),
    Tc: 30,
    pc: 31,
  };

  test("are the package's: each region its phase and outline, the critical lines in runs, the critical point", () => {
    expect(readRegions(regions)).toEqual({
      regions: [
        { phase: "gas", outline: [corner([1, 2, 3, 4, 5]), corner([6, 7, 8, 9, 10])] },
        { phase: "liquid", outline: [corner([11, 12, 13, 14, 15])] },
      ],
      isotherm: [[corner([20, 21, 22, 23, 24])], [corner([25, 26, 27, 28, 29])]],
      isobar: [],
      critical: corner([30, 31, 32, 33, 34]),
      Tc: 30,
      pc: 31,
    });
  });

  test("without a critical point have none", () => {
    expect(readRegions({ ...regions, critical: new Float64Array() }).critical).toBeNull();
  });
});

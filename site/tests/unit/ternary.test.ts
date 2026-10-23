// The ternary page's geometry (PLAN.md M26.6): src/ternary/geometry.ts reads the package's diagram and places its
// points on the triangle, on a hand-made answer whose every number the test can follow.
import { describe, expect, test } from "vitest";
import type { TernaryValue } from "../../src/engine/protocol";
import { boxHeight, fractionsAt, gridLines, place, readTernary, triangle } from "../../src/ternary/geometry";

const VALUE: TernaryValue = {
  components: ["A", "B", "C"],
  T: 273.15,
  p: 5e5,
  lines: new Uint32Array([0, 2, 3]),
  binodal: new Float64Array([0.3, 0, 0.7, 0.3, 0.7, 0, 0.6, 0, 0.4]),
  binodalBeta: new Float64Array([0.01, 0.2, 0.99]),
  outside: new Float64Array(9),
  feeds: new Float64Array([0.45, 0, 0.55]),
  beta: new Float64Array([0.5]),
  liquid: new Float64Array([0.3, 0, 0.7]),
  vapour: new Float64Array([0.6, 0, 0.4]),
  polygons: new Uint32Array([0, 3, 7]),
  region: new Float64Array([
    0.3, 0, 0.7, 0.6, 0, 0.4, 0.3, 0.1, 0.6, 0.3, 0.1, 0.6, 0.6, 0, 0.4, 0.5, 0.1, 0.4, 0.4, 0.2, 0.4,
  ]),
  feed: [0.4, 0.2, 0.4],
};

describe("the ternary's reader", () => {
  test("gives each binodal line its points and side, and each tie line its phases", () => {
    const d = readTernary(VALUE);
    expect([d.components, d.T, d.p, d.feed]).toEqual([["A", "B", "C"], 273.15, 5e5, [0.4, 0.2, 0.4]]);
    expect(d.binodal).toEqual([
      {
        points: [
          [0.3, 0, 0.7],
          [0.3, 0.7, 0],
        ],
        bubble: true,
      },
      { points: [[0.6, 0, 0.4]], bubble: false },
    ]);
    expect(d.ties).toEqual([{ liquid: [0.3, 0, 0.7], vapour: [0.6, 0, 0.4] }]);
    expect(d.region.map((r) => r.length)).toEqual([3, 4]);
    expect(d.region[1]?.[3]).toEqual([0.4, 0.2, 0.4]);
  });

  test("marks no composition where the mixture's is not three fractions, and a line of half its β on the dew side", () => {
    const d = readTernary({ ...VALUE, feed: [], binodalBeta: new Float64Array([0.5, 0.5, 0.5]) });
    expect(d.feed).toBeNull();
    expect(d.binodal.map((l) => l.bubble)).toEqual([false, false]);
  });
});

describe("the triangle", () => {
  const t = triangle(520);

  test("fits its box, its side at most 640 px and at least 200", () => {
    expect(t).toEqual({ side: 400, left: 60, bottom: 200 * Math.sqrt(3) + 36 });
    expect(boxHeight(t)).toBe(t.bottom + 40);
    expect(triangle(2000).side).toBe(640);
    expect(triangle(100).side).toBe(200);
  });

  test("puts the first component at the bottom right, the second at the top and the third at the bottom left", () => {
    expect(place([1, 0, 0], t)).toEqual([460, t.bottom]);
    expect(place([0, 0, 1], t)).toEqual([60, t.bottom]);
    const [x, y] = place([0, 1, 0], t);
    expect([x, y]).toEqual([260, t.bottom - 200 * Math.sqrt(3)]);
  });

  test("reads back the fractions it placed, and none outside", () => {
    const [x, y] = place([0.2, 0.3, 0.5], t);
    const back = fractionsAt(x, y, t) ?? [NaN, NaN, NaN];
    [0.2, 0.3, 0.5].forEach((v, i) => expect(back[i]).toBeCloseTo(v, 12));
    expect(fractionsAt(40, t.bottom - 1, t)).toBeNull();
    expect(fractionsAt(470, t.bottom - 1, t)).toBeNull();
    expect(fractionsAt(260, t.bottom + 5, t)).toBeNull();
  });

  test("draws its grid at every tenth of each fraction, side to side", () => {
    const lines = gridLines(t);
    expect(lines).toHaveLength(27);
    expect(lines[0]).toEqual([place([0.1, 0, 0.9], t), place([0.1, 0.9, 0], t)]);
  });
});

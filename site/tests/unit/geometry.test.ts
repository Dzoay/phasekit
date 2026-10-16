// The phase regions' plane geometry (PLAN.md M21.9): inside a polygon by the even-odd rule, the distance to its edges,
// its part inside an area, and the points of an area deepest inside it.
import { describe, expect, test } from "vitest";
import { clearance, clip, deepest, edges, inside, thin, toSegment, type Point } from "../../src/diagram/geometry";

const square: Point[] = [
  [0, 0],
  [4, 0],
  [4, 4],
  [0, 4],
];
// An L: the square's lower-left quarter-wide strips.
const ell: Point[] = [
  [0, 0],
  [4, 0],
  [4, 1],
  [1, 1],
  [1, 4],
  [0, 4],
];

describe("a polygon", () => {
  test("holds the points inside it, a concave one too, by the even-odd rule", () => {
    expect(inside([2, 2], square)).toBe(true);
    expect(inside([5, 2], square)).toBe(false);
    expect(inside([-1, 2], square)).toBe(false);
    expect(inside([2, 5], square)).toBe(false);
    expect(inside([0.5, 3], ell)).toBe(true);
    expect(inside([3, 0.5], ell)).toBe(true);
    expect(inside([3, 3], ell)).toBe(false);
    expect(inside([2, 2], [])).toBe(false);
    // A point level with a vertex: the edges meeting there count once between them.
    const kite: Point[] = [
      [0, 0],
      [4, 2],
      [0, 4],
    ];
    expect(inside([1, 2], kite)).toBe(true);
    expect(inside([5, 2], kite)).toBe(false);
    // On an edge: the left one counts as inside, the right one not.
    expect(inside([0, 2], square)).toBe(true);
    expect(inside([4, 2], square)).toBe(false);
    // A polygon that winds around twice counts its doubled part as outside.
    const twice: Point[] = [...square, [0, 0], [2, 0], [2, 2], [0, 2]];
    expect(inside([1, 1], twice)).toBe(false);
    expect(inside([3, 3], twice)).toBe(true);
  });

  test("has its edges, the closing one first", () => {
    expect(
      edges([
        [0, 0],
        [1, 0],
        [1, 1],
      ]),
    ).toEqual([
      [
        [1, 1],
        [0, 0],
      ],
      [
        [0, 0],
        [1, 0],
      ],
      [
        [1, 0],
        [1, 1],
      ],
    ]);
  });

  test("measures the squared distance to a segment, past its ends and on a point", () => {
    expect(toSegment([0, 1], [-1, 0], [1, 0])).toBe(1);
    expect(toSegment([3, 0], [-1, 0], [1, 0])).toBe(4);
    expect(toSegment([-3, 2], [-1, 0], [1, 0])).toBe(8);
    expect(toSegment([4, 5], [1, 1], [1, 1])).toBe(25);
    expect(toSegment([2, 1], [0, 0], [4, 0])).toBe(1);
  });

  test("measures the distance to its nearest edge, Infinity with none", () => {
    expect(clearance([1, 2], edges(square))).toBe(1);
    expect(clearance([2, 2], edges(square))).toBe(2);
    expect(clearance([2, 3], edges(ell))).toBe(1);
    expect(clearance([1, 2], [])).toBe(Infinity);
  });
});

describe("the deepest points", () => {
  const area = { left: 0, top: 0, right: 4, bottom: 4 };

  test("are the grid's points inside the polygon, deepest first by the polygon's edges and the area's sides", () => {
    // The grid's centres are 0.5, 1.5, 2.5 and 3.5 on each axis; the square's own sides are the area's.
    const found = deepest(square, area, 4, 4);
    expect(found.length).toBe(16);
    expect(found.slice(0, 4)).toEqual([
      [1.5, 1.5],
      [1.5, 2.5],
      [2.5, 1.5],
      [2.5, 2.5],
    ]);
    // Only those at least `least` deep: the centre four are 1.5 deep, the rest 0.5.
    expect(deepest(square, area, 4, 4, 1.5)).toEqual(found.slice(0, 4));
    expect(deepest(square, area, 4, 4, 1.6)).toEqual([]);
    expect(deepest(square, area, 4, 4, 0.5).length).toBe(16);
    expect(deepest(ell, area, 4, 4)).toEqual([
      [0.5, 0.5],
      [0.5, 1.5],
      [0.5, 2.5],
      [0.5, 3.5],
      [1.5, 0.5],
      [2.5, 0.5],
      [3.5, 0.5],
    ]);
  });

  test("of a clipped polygon are measured to its edges, the area's sides among them", () => {
    // A triangle whose long side, x + y = 6, cuts the area's corner; its other sides lie far outside it.
    const triangle: Point[] = [
      [-10, -10],
      [16, -10],
      [-10, 16],
    ];
    const found = deepest(clip(triangle, area), area, 4, 4);
    expect(found.slice(0, 3)).toEqual([
      [1.5, 1.5],
      [1.5, 2.5],
      [2.5, 1.5],
    ]);
    expect(found).not.toContainEqual([3.5, 3.5]);
    // Unclipped, the triangle's own sides are far, and the long one nearest the area's lower left.
    expect(deepest(triangle, area, 4, 4)[0]).toEqual([0.5, 0.5]);
    // The default grid is 32 × 20.
    const big: Point[] = [
      [-1, -1],
      [33, -1],
      [33, 21],
      [-1, 21],
    ];
    expect(deepest(big, { left: 0, top: 0, right: 32, bottom: 20 }).length).toBe(640);
    expect(deepest(square, area, 2, 1)).toEqual([
      [1, 2],
      [3, 2],
    ]);
    // An area away from the origin.
    const away: Point[] = [
      [10, 20],
      [14, 20],
      [14, 24],
      [10, 24],
    ];
    expect(deepest(away, { left: 10, top: 20, right: 14, bottom: 24 }, 2, 1)).toEqual([
      [11, 22],
      [13, 22],
    ]);
  });
});

describe("clipping", () => {
  const area = { left: 0, top: 0, right: 8, bottom: 8 };

  test("keeps a polygon inside the area as it is, and nothing of one outside it", () => {
    expect(clip(square, area)).toEqual(square);
    expect(
      clip(
        [
          [10, 0],
          [12, 0],
          [12, 2],
        ],
        area,
      ),
    ).toEqual([]);
    expect(clip([], area)).toEqual([]);
  });

  test("cuts a polygon at each side it crosses, on the side exactly", () => {
    const big: Point[] = [
      [-1, -1],
      [9, -1],
      [9, 9],
      [-1, 9],
    ];
    expect(clip(big, area)).toEqual([
      [0, 8],
      [0, 0],
      [8, 0],
      [8, 8],
    ]);
    // x + y = 4 from (4, 0) to (-4, 8) crosses the left side at y = 4.
    expect(
      clip(
        [
          [-4, 0],
          [4, 0],
          [-4, 8],
        ],
        area,
      ),
    ).toEqual([
      [0, 0],
      [4, 0],
      [0, 4],
    ]);
    // Through the top and the bottom.
    expect(
      clip(
        [
          [2, -4],
          [6, -4],
          [6, 12],
          [2, 12],
        ],
        area,
      ),
    ).toEqual([
      [2, 8],
      [2, 0],
      [6, 0],
      [6, 8],
    ]);
    // Through the bottom, slanting: (6, 2) to (2, 10) crosses y = 8 at x = 3.
    expect(
      clip(
        [
          [2, 2],
          [6, 2],
          [2, 10],
        ],
        area,
      ),
    ).toEqual([
      [2, 8],
      [2, 2],
      [6, 2],
      [3, 8],
    ]);
    // Through the right side, slanting: (6, 2) to (10, 6) crosses x = 8 at y = 4.
    expect(
      clip(
        [
          [2, 2],
          [6, 2],
          [10, 6],
          [2, 6],
        ],
        area,
      ),
    ).toEqual([
      [2, 2],
      [6, 2],
      [8, 4],
      [8, 6],
      [2, 6],
    ]);
  });
});

describe("thinning", () => {
  test("drops the vertices within the gap of the last one kept, by |dx| + |dy|", () => {
    const outline: Point[] = [
      [0, 0],
      [1, 0],
      [2, 0],
      [2, 1],
      [3, 1.5],
      [3, 3],
    ];
    expect(thin(outline, 2)).toEqual([
      [0, 0],
      [2, 0],
      [3, 1.5],
    ]);
    expect(thin(outline, 0)).toEqual(outline);
    expect(thin([], 2)).toEqual([]);
  });
});

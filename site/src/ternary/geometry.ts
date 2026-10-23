// A ternary diagram as the page draws it (PLAN.md M26.6): the package's binodal, tie lines and two-phase region
// (phasekit-diagram's M26.5, through `Registry.ternary`), each point three mole fractions, placed on an equilateral
// triangle: the first component at the bottom right corner, the second at the top, the third at the bottom left. It
// adds no numerics but that placement.
import type { TernaryValue } from "../engine/protocol";

export type Fractions = readonly [number, number, number];

/** A binodal line: its points, and whether it is the bubble side, where the feed is nearly all liquid. */
export interface BinodalLine {
  points: Fractions[];
  bubble: boolean;
}

export interface TernaryData {
  components: string[];
  T: number;
  p: number;
  feed: Fractions | null;
  binodal: BinodalLine[];
  ties: { liquid: Fractions; vapour: Fractions }[];
  region: Fractions[][];
}

function point(values: Float64Array, k: number): Fractions {
  return [values[3 * k] as number, values[3 * k + 1] as number, values[3 * k + 2] as number];
}

/** The package's ternary diagram as the page draws it; a line is the bubble side where its points' mean vapour
 *  fraction is below a half. */
export function readTernary(t: TernaryValue): TernaryData {
  const binodal = Array.from({ length: t.lines.length - 1 }, (_, k) => {
    const [a, b] = [t.lines[k] as number, t.lines[k + 1] as number];
    const points = Array.from({ length: b - a }, (_, i) => point(t.binodal, a + i));
    const beta = Array.from(t.binodalBeta.subarray(a, b));
    const mean = beta.reduce((s, v) => s + v, 0) / Math.max(1, beta.length);
    return { points, bubble: mean < 0.5 };
  });
  const ties = Array.from(t.beta, (_, j) => ({ liquid: point(t.liquid, j), vapour: point(t.vapour, j) }));
  const region = Array.from({ length: t.polygons.length - 1 }, (_, i) => {
    const [a, b] = [t.polygons[i] as number, t.polygons[i + 1] as number];
    return Array.from({ length: b - a }, (_, k) => point(t.region, a + k));
  });
  const feed = t.feed.length === 3 ? ([t.feed[0], t.feed[1], t.feed[2]] as Fractions) : null;
  return { components: t.components, T: t.T, p: t.p, feed, binodal, ties, region };
}

/** The triangle's frame: its side in px and its bottom left corner. */
export interface Triangle {
  side: number;
  left: number;
  bottom: number;
}

/** The triangle for a box `width` px wide, with room for the corners' names. */
export function triangle(width: number): Triangle {
  const side = Math.max(200, Math.min(width - 120, 640));
  return { side, left: (width - side) / 2, bottom: (side * Math.sqrt(3)) / 2 + 36 };
}

/** The height of the box the triangle needs, its names below and above it included. */
export const boxHeight = (t: Triangle): number => t.bottom + 40;

/** Where a composition lies: the first fraction toward the bottom right, the second up. */
export function place([a, b]: Fractions, t: Triangle): [number, number] {
  return [t.left + (a + b / 2) * t.side, t.bottom - ((b * Math.sqrt(3)) / 2) * t.side];
}

/** The composition at a point of the triangle, the inverse of `place`; null outside it. */
export function fractionsAt(x: number, y: number, t: Triangle): Fractions | null {
  const b = (t.bottom - y) / ((Math.sqrt(3) / 2) * t.side);
  const a = (x - t.left) / t.side - b / 2;
  const c = 1 - a - b;
  return a < 0 || b < 0 || c < 0 ? null : [a, b, c];
}

/** The triangle's grid at every tenth of each fraction, as segments between its sides. */
export function gridLines(t: Triangle): [[number, number], [number, number]][] {
  const out: [[number, number], [number, number]][] = [];
  for (let k = 1; k < 10; k++) {
    const f = k / 10;
    out.push([place([f, 0, 1 - f], t), place([f, 1 - f, 0], t)]);
    out.push([place([0, f, 1 - f], t), place([1 - f, f, 0], t)]);
    out.push([place([0, 1 - f, f], t), place([1 - f, 0, f], t)]);
  }
  return out;
}

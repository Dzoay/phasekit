// Plane geometry for the phase regions' labels (PLAN.md M21.9): whether a point is inside a polygon, how far it is
// from the polygon's edges, and the point of an area deepest inside a polygon.

export type Point = readonly [number, number];

export interface Area {
  left: number;
  top: number;
  right: number;
  bottom: number;
}

/** Whether `p` is inside the polygon, by the even-odd rule; the polygon closes itself. */
export function inside(p: Point, polygon: readonly Point[]): boolean {
  // Indexed, not destructured: this loop runs for every grid point and edge of a label's search.
  const x = p[0];
  const y = p[1];
  let within = false;
  for (let i = 0, j = polygon.length - 1; i < polygon.length; j = i++) {
    const a = polygon[i] as Point;
    const b = polygon[j] as Point;
    const crosses = a[1] > y !== b[1] > y;
    // Stryker disable next-line EqualityOperator: a ray to the left (x ≥ the crossing) crosses as often, modulo 2, as one to the right, and x ≤ differs only for a point on an edge, which no search asks about.
    if (crosses && x < a[0] + ((b[0] - a[0]) * (y - a[1])) / (b[1] - a[1])) within = !within;
  }
  return within;
}

/** The squared distance from `p` to the segment from `a` to `b`. */
export function toSegment(p: Point, a: Point, b: Point): number {
  const dx = b[0] - a[0];
  const dy = b[1] - a[1];
  const length = dx * dx + dy * dy;
  const t = length === 0 ? 0 : Math.min(1, Math.max(0, ((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / length));
  const ex = p[0] - a[0] - t * dx;
  const ey = p[1] - a[1] - t * dy;
  return ex * ex + ey * ey;
}

/** The polygon's edges, its closing edge included. */
export function edges(polygon: readonly Point[]): [Point, Point][] {
  return polygon.map((p, i) => [polygon.at(i - 1) as Point, p]);
}

/** The distance from `p` to the nearest of `segments`; Infinity if there are none. */
export function clearance(p: Point, segments: readonly (readonly [Point, Point])[]): number {
  let nearest = Infinity;
  for (const segment of segments) nearest = Math.min(nearest, toSegment(p, segment[0], segment[1]));
  return Math.sqrt(nearest);
}

/** The polygon without the vertices that lie within `gap` of the last one kept: an outline sampled for its curves,
 *  thinned for a search at the scale of a label. */
export function thin(polygon: readonly Point[], gap: number): Point[] {
  const out: Point[] = [];
  for (const p of polygon) {
    const last = out.at(-1);
    if (!last || Math.abs(p[0] - last[0]) + Math.abs(p[1] - last[1]) >= gap) out.push(p);
  }
  return out;
}

/** The part of the polygon inside the area (Sutherland and Hodgman's clipping, one side of the area at a time). A
 *  concave polygon's parts come out joined along the area's sides, by edges that enclose nothing. */
export function clip(polygon: readonly Point[], area: Area): Point[] {
  const { left, top, right, bottom } = area;
  // Where a segment crosses a vertical or a horizontal line, that line's coordinate exactly.
  const atX =
    (x: number) =>
    (a: Point, b: Point): Point => [x, a[1] + ((x - a[0]) * (b[1] - a[1])) / (b[0] - a[0])];
  const atY =
    (y: number) =>
    (a: Point, b: Point): Point => [a[0] + ((y - a[1]) * (b[0] - a[0])) / (b[1] - a[1]), y];
  // Each side: whether a point is on the area's side of it, and where a segment crosses it. A point on a side is kept
  // either way, as the crossing there is the point itself.
  const sides: [(p: Point) => boolean, (a: Point, b: Point) => Point][] = [
    // Stryker disable next-line EqualityOperator: a point on the side is its own crossing.
    [(p) => p[0] >= left, atX(left)],
    // Stryker disable next-line EqualityOperator: as above.
    [(p) => p[0] <= right, atX(right)],
    // Stryker disable next-line EqualityOperator: as above.
    [(p) => p[1] >= top, atY(top)],
    // Stryker disable next-line EqualityOperator: as above.
    [(p) => p[1] <= bottom, atY(bottom)],
  ];
  let out: Point[] = [...polygon];
  for (const [keeps, cross] of sides) {
    const input = out;
    out = [];
    input.forEach((b, i) => {
      const a = input.at(i - 1) as Point;
      if (keeps(b) !== keeps(a)) out.push(cross(a, b));
      if (keeps(b)) out.push(b);
    });
  }
  return out;
}

/** The points of a `columns` × `rows` grid over the area that are inside the polygon and at least `least` from its
 *  edges, deepest first. */
export function deepest(polygon: readonly Point[], area: Area, columns = 32, rows = 20, least = 0): Point[] {
  const { left, top, right, bottom } = area;
  const sides = edges(polygon);
  const found: { p: Point; depth: number }[] = [];
  for (let i = 0; i < columns; i++) {
    for (let j = 0; j < rows; j++) {
      const p: Point = [left + ((i + 0.5) * (right - left)) / columns, top + ((j + 0.5) * (bottom - top)) / rows];
      const depth = inside(p, polygon) ? clearance(p, sides) : -1;
      if (depth >= least) found.push({ p, depth });
    }
  }
  return found.sort((a, b) => b.depth - a.depth).map((f) => f.p);
}

// The curves page's two charts (PLAN.md M26.6; user decision DG10), as paths in pixels the page draws in SVG: the
// characteristic curves whole, log p against log T up to the equation of state's highest temperature, over the
// saturation line; and the compressibility chart, Z against log p, an isotherm a line. Every point is the package's;
// this only scales them.
import { scaleLinear, scaleLog } from "d3-scale";
import { decadeText, pathOf } from "../diagram/model";
import { CURVES, type ChartIsotherm, type Curve, type CurveState } from "../diagram/overlays";
import type { Vertex } from "../diagram/data";
import { formatNumber } from "../lib/format";

export const MARGIN = { left: 64, top: 12, right: 84, bottom: 44 } as const;

export interface Frame {
  width: number;
  height: number;
  left: number;
  top: number;
  right: number;
  bottom: number;
}

export function frame(width: number, height: number): Frame {
  return {
    width,
    height,
    left: MARGIN.left,
    top: MARGIN.top,
    right: width - MARGIN.right,
    bottom: height - MARGIN.bottom,
  };
}

export interface Path {
  d: string;
  colour: string;
  dash: string;
  width: number;
  /** Where its label goes, at its last point, and what it reads; none for a line without one. */
  label: { x: number; y: number; text: string } | null;
}

export interface Tick {
  at: number;
  text: string;
}

export interface Chart {
  frame: Frame;
  paths: Path[];
  dots: { x: number; y: number }[];
  xTicks: Tick[];
  yTicks: Tick[];
}

/** The least and greatest of the positive finite values, or null where there is none. */
export function extent(values: Iterable<number>): [number, number] | null {
  let [lo, hi] = [Infinity, -Infinity];
  for (const v of values) {
    if (Number.isFinite(v) && v > 0) [lo, hi] = [Math.min(lo, v), Math.max(hi, v)];
  }
  return lo <= hi ? [lo, hi] : null;
}

type Scale = (v: number) => number;

/** A log scale's decade ticks inside its domain, or its own ticks where it spans less than a decade. */
function logTicks(scale: ReturnType<typeof scaleLog<number>>): Tick[] {
  const [lo, hi] = scale.domain() as [number, number];
  const decades: number[] = [];
  for (let k = Math.ceil(Math.log10(lo)); k <= Math.floor(Math.log10(hi)); k++) decades.push(10 ** k);
  const decade = decades.length >= 2;
  return (decade ? decades : scale.ticks(4)).map((v) => ({
    at: scale(v),
    text: decade ? decadeText(v) : formatNumber(v),
  }));
}

function path(points: [number, number][], style: Omit<Path, "d" | "label">, label: string | null): Path {
  const last = points.at(-1);
  return { d: pathOf(points), ...style, label: label && last ? { x: last[0], y: last[1], text: label } : null };
}

/** The curves whole on log p against log T, over the saturation line and its critical point. */
export function curvesChart(
  curves: readonly Curve[],
  saturation: readonly Vertex[],
  critical: Vertex | null,
  f: Frame,
  colour: (token: string) => string,
): Chart {
  const states = curves.flatMap((c) => c.lines.flat());
  const t = extent([...states.map((s) => s.T), ...saturation.map((v) => v.x)]) ?? [1, 10];
  const p = extent([...states.map((s) => s.p), ...saturation.map((v) => v.y)]) ?? [1, 10];
  const x = scaleLog().domain(t).range([f.left, f.right]);
  const y = scaleLog().domain(p).range([f.bottom, f.top]);
  const at = (T: number, P: number): [number, number] => [x(T), y(P)];
  const paths: Path[] = [
    path(
      saturation.map((v) => at(v.x, v.y)),
      { colour: colour("--fg"), dash: "", width: 2 },
      null,
    ),
  ];
  for (const curve of curves) {
    const style = CURVES[curve.name];
    if (!style) continue;
    for (const line of curve.lines) {
      const points = line.map((s) => at(s.T, s.p));
      paths.push(path(points, { colour: colour(style.colour), dash: style.dash.join(" "), width: 2 }, null));
    }
  }
  return {
    frame: f,
    paths,
    dots: critical ? [{ x: x(critical.x), y: y(critical.y) }] : [],
    xTicks: logTicks(x),
    yTicks: logTicks(y),
  };
}

/** The isotherm's colour token: the sequential ramp's six steps spread over the isotherms, coldest first. */
export function rampToken(j: number, n: number): string {
  const step = n > 1 ? Math.round((j / (n - 1)) * 5) : 0;
  return `--seq-${step + 1}`;
}

/** The compressibility chart: Z against log p, each isotherm labelled with its temperature at its high-pressure end. */
export function zChart(chart: readonly ChartIsotherm[], f: Frame, colour: (token: string) => string): Chart {
  const states: CurveState[] = chart.flatMap((i) => i.runs.flat());
  const p = extent(states.map((s) => s.p)) ?? [1, 10];
  const zs = states.map((s) => s.Z);
  const z: [number, number] = [Math.min(0, ...zs), Math.max(1.1, ...zs)];
  const x = scaleLog().domain(p).range([f.left, f.right]);
  const y = scaleLinear().domain(z).range([f.bottom, f.top]).nice();
  const at = (s: CurveState): [number, number] => [x(s.p), (y as Scale)(s.Z)];
  const paths = chart.flatMap((isotherm, j) => {
    const style = { colour: colour(rampToken(j, chart.length)), dash: "", width: 1.5 };
    return isotherm.runs.map((run, k) =>
      path(run.map(at), style, k === isotherm.runs.length - 1 ? `${formatNumber(isotherm.T)} K` : null),
    );
  });
  return {
    frame: f,
    paths: apart(paths),
    dots: [],
    xTicks: logTicks(x),
    yTicks: y.ticks(6).map((v) => ({ at: y(v), text: formatNumber(v) })),
  };
}

/** The labels' least vertical distance apart, px: a label nearer one kept already is dropped, its line kept. */
const LABEL_GAP = 12;

/** The paths with only those labels kept that are `LABEL_GAP` apart, taken in order. */
export function apart(paths: Path[]): Path[] {
  // Stryker disable next-line ArrayDeclaration: a stray entry that is no number is no label's height, and keeps none away.
  const kept: number[] = [];
  return paths.map((p) => {
    if (!p.label) return p;
    const y = p.label.y;
    if (kept.some((k) => Math.abs(k - y) < LABEL_GAP)) return { ...p, label: null };
    kept.push(y);
    return p;
  });
}

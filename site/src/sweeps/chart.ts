// The sweeps' chart (PLAN.md M21.3): scales in the shown units, ticks, each line in its colour and dash as segments
// broken at failed cells, a label at each line's last point in the plot, and the sample nearest the cursor. Isobars and
// isotherms are ordered by their held value, so they take a sequential ramp, lightest to darkest in the light theme;
// the two saturated phases take the isolines' first two colours (the dataviz skill).
import { scaleLinear, scaleLog } from "d3-scale";
import { decadeText, place, type Box, type Label, type Point } from "../diagram/model";
import { MARGIN, padded, type Plot, type Scale } from "../diagram/scales";
import { lineLabel } from "../diagram/style";
import { formatNumber } from "../lib/format";
import { output, type Output } from "../lib/outputs";
import { fromSI, type System } from "../lib/units";
import { LATENT, type Sweep, type SweepLine } from "./sample";
import { unitOfOutput } from "./view.svelte";

/** The sequential ramp's tokens, one a line, in the order of the lines' held values. */
export const RAMP = ["--seq-1", "--seq-2", "--seq-3", "--seq-4", "--seq-5", "--seq-6"] as const;

const PHASE_STYLES = [
  { colour: "--iso-1", dash: [] },
  { colour: "--iso-2", dash: [6, 4] },
] as const;

export interface ChartLine {
  name: string;
  colour: string;
  dash: readonly number[];
  segments: Point[][];
}

export interface Chart {
  plot: Plot;
  x: Scale;
  y: Scale;
  lines: ChartLine[];
  labels: Label[];
  xTicks: { at: number; text: string }[];
  yTicks: { at: number; text: string }[];
  titles: { x: string; y: string };
}

/** A line's name: its held value in the shown units, its phase, or the latent heat's difference. */
export function lineName(line: SweepLine, system: System): string {
  if (line.held === null) return "h″ − h′";
  if (line.held === "Q") return line.value === 0 ? "liquid" : "vapour";
  return lineLabel(line.held, line.value, system);
}

/** A line's colour token and dash: a saturated phase's own, or the ramp's step for its held value's rank. */
export function lineStyle(sweep: Sweep, index: number): { colour: string; dash: readonly number[] } {
  const line = sweep.lines[index] as SweepLine;
  if (line.held === null || line.held === "Q") return PHASE_STYLES[index] as (typeof PHASE_STYLES)[number];
  const rank = sweep.lines.filter((l) => l.value < line.value).length;
  return { colour: RAMP[rank] as string, dash: [] };
}

/** An axis's domain from its values, padded; a single value is widened by a tenth of itself, or by one at zero. */
function domain(values: number[], log: boolean): [number, number] {
  if (values.length === 0) return log ? [1, 10] : [0, 1];
  const [lo, hi] = [Math.min(...values), Math.max(...values)];
  if (lo === hi) return padded(lo === 0 ? [-1, 1] : [lo - Math.abs(lo) / 10, hi + Math.abs(hi) / 10], log);
  return padded([lo, hi], log);
}

export interface ChartOptions {
  width: number;
  height: number;
  system: System;
  /** A colour token resolved in the current theme. */
  colour: (token: string) => string;
}

export function buildChart(sweep: Sweep, y: string, o: ChartOptions): Chart {
  const plot = { left: MARGIN.left, top: MARGIN.top, right: o.width - MARGIN.right, bottom: o.height - MARGIN.bottom };
  const log = sweep.x === "P";
  const [ux, uy] = [unitOfOutput(sweep.x, o.system), unitOfOutput(y, o.system)];
  const shown = sweep.lines.flatMap((l) => l.points.flatMap((p) => (p ? [[fromSI(p.x, ux), fromSI(p.y, uy)]] : [])));
  const xs = (log ? scaleLog() : scaleLinear())
    .domain(
      domain(
        shown.map(([v]) => v as number),
        log,
      ),
    )
    .range([plot.left, plot.right]);
  const ys = scaleLinear()
    .domain(
      domain(
        shown.map(([, v]) => v as number),
        false,
      ),
    )
    .range([plot.bottom, plot.top]);
  const at = (p: { x: number; y: number }): Point => [xs(fromSI(p.x, ux)), ys(fromSI(p.y, uy))];
  // Stryker disable next-line ArrayDeclaration: a stray entry that is no box overlaps nothing.
  const placed: Box[] = [];
  const labels: Label[] = [];
  const lines = sweep.lines.map((line, i) => {
    const style = lineStyle(sweep, i);
    const segments: Point[][] = [[]];
    for (const p of line.points) {
      if (p) (segments.at(-1) as Point[]).push(at(p));
      else segments.push([]);
    }
    const drawn = segments.filter((s) => s.length > 1);
    const name = lineName(line, o.system);
    const ends = [drawn.at(-1)?.at(-1), drawn[0]?.[0]].filter((e): e is Point => e !== undefined);
    const label = place(name, ends, plot, placed);
    if (label) labels.push(label);
    return { name, colour: o.colour(style.colour), dash: style.dash, segments: drawn };
  });
  const xTicks = (log ? xs.ticks().filter((v) => Number.isInteger(Math.log10(v))) : xs.ticks(6)).map((v) => ({
    at: xs(v),
    text: log ? decadeText(v) : formatNumber(v),
  }));
  const yTicks = ys.ticks(6).map((v) => ({ at: ys(v), text: formatNumber(v) }));
  const symbol = (key: string) => (key === LATENT ? "h″ − h′" : (output(key) as Output).symbol);
  return {
    plot,
    x: xs,
    y: ys,
    lines,
    labels,
    xTicks,
    yTicks,
    titles: { x: `${symbol(sweep.x)} / ${ux.label}`, y: `${symbol(y)} / ${uy.label}` },
  };
}

/** The index of the sample nearest `x` (in SI; in log space on a log axis) among those any line has; -1 for none. */
export function nearest(sweep: Sweep, x: number): number {
  const log = sweep.x === "P";
  const distance = (v: number) => (log ? Math.abs(Math.log(v / x)) : Math.abs(v - x));
  let best = -1;
  let gap = Infinity;
  const count = Math.max(0, ...sweep.lines.map((l) => l.points.length));
  // Stryker disable next-line EqualityOperator: past the last sample no line has a point.
  for (let i = 0; i < count; i++) {
    const p = sweep.lines.map((l) => l.points[i]).find((q) => q);
    if (p && distance(p.x) < gap) [best, gap] = [i, distance(p.x)];
  }
  return best;
}

// What a diagram draws (PLAN.md M21.2), as one model the canvas paints and the SVG export writes: the dome, each shown
// line in its family's style, the critical point, a label at each line's last point in the plot, the ticks and the
// axis titles. And the CSV of its vertices.
import { formatNumber, superscript } from "../lib/format";
import { output, type Output } from "../lib/outputs";
import { fromSI, type System } from "../lib/units";
import type { Diagram, Key } from "../sample/diagrams";
import type { DiagramData, Vertex } from "../sample/lines";
import type { Plot, Scale } from "./scales";
import { lineLabel, styleOf, unitOfKey } from "./style";

export type Point = [number, number];

export interface Stroke {
  points: Point[];
  colour: string;
  dash: readonly number[];
  width: number;
}

export interface Model {
  plot: Plot;
  strokes: Stroke[];
  dots: { x: number; y: number; r: number; colour: string }[];
  labels: Label[];
  xTicks: { at: number; text: string }[];
  yTicks: { at: number; text: string }[];
  titles: { x: string; y: string };
}

/** A label's text anchor point, on the side of its line's point where it fits. */
export interface Label {
  x: number;
  y: number;
  text: string;
  anchor: "start" | "end";
}

/** A label's estimated size, for keeping labels apart: px per character of the 11 px figures, and their height. */
const CHAR = 6.5;
const HEIGHT = 10;
const GAP = 4;

export interface Box {
  x0: number;
  x1: number;
  y0: number;
  y1: number;
}

/** Whether two label boxes overlap; boxes that only touch do not. */
export function overlaps(a: Box, b: Box): boolean {
  return a.x0 < b.x1 && b.x0 < a.x1 && a.y0 < b.y1 && b.y0 < a.y1;
}

/** A label for `text` at the first of `candidates` where it overlaps no label already placed, beside its point:
 *  above it unless that leaves the plot's top, right-anchored where it would leave the plot's right side. */
export function place(text: string, candidates: readonly Point[], plot: Plot, placed: Box[]): Label | null {
  const width = text.length * CHAR;
  for (const [px, py] of candidates) {
    const anchor = px + GAP + width > plot.right ? "end" : "start";
    const x = anchor === "start" ? px + GAP : px - GAP;
    const y = py - GAP - HEIGHT < plot.top ? py + GAP + HEIGHT : py - GAP;
    const box = {
      x0: anchor === "start" ? x : x - width,
      x1: anchor === "start" ? x + width : x,
      y0: y - HEIGHT,
      y1: y,
    };
    if (!placed.some((b) => overlaps(box, b))) {
      placed.push(box);
      return { x, y, text, anchor };
    }
  }
  return null;
}

export interface Options {
  x: Scale;
  y: Scale;
  plot: Plot;
  system: System;
  /** The families the viewer has turned off. */
  hidden: ReadonlySet<Key>;
  /** A colour token resolved in the current theme. */
  colour: (token: string) => string;
}

/** A decade tick: plain from a thousandth to a thousand, a power of ten past that. */
export function decadeText(v: number): string {
  const n = Math.round(Math.log10(v));
  return Math.abs(n) <= 3 ? String(10 ** n) : `10${superscript(n)}`;
}

/** An axis's ticks inside [lo, hi] px, thinned so that each keeps `gap(text)` px from the one before it. */
function ticks(
  scale: Scale,
  log: boolean,
  lo: number,
  hi: number,
  gap: (text: string) => number,
): { at: number; text: string }[] {
  const values = log
    ? // Stryker disable next-line EqualityOperator: no tick sits exactly 1e-9 of a decade from a power of ten.
      scale.ticks().filter((v) => Math.abs(Math.log10(v) - Math.round(Math.log10(v))) < 1e-9)
    : scale.ticks(6);
  const kept: { at: number; text: string }[] = [];
  for (const t of values.map((v) => ({ at: scale(v), text: log ? decadeText(v) : formatNumber(v) }))) {
    const before = kept.at(-1);
    if (lo <= t.at && t.at <= hi && (!before || Math.abs(t.at - before.at) >= gap(before.text))) kept.push(t);
  }
  return kept;
}

export function buildModel(d: Diagram, data: DiagramData, o: Options): Model {
  const [ux, uy] = [unitOfKey(d.x.key, o.system), unitOfKey(d.y.key, o.system)];
  const px = (v: Vertex): Point => [o.x(fromSI(v.x, ux)), o.y(fromSI(v.y, uy))];
  const inside = ([x, y]: Point) => o.plot.left <= x && x <= o.plot.right && o.plot.top <= y && y <= o.plot.bottom;
  const ink = o.colour("--fg");
  const { liquid, vapour, critical } = data.dome;
  const strokes: Stroke[] = [
    {
      points: [...liquid, ...(critical ? [critical] : []), ...[...vapour].reverse()].map(px),
      colour: ink,
      dash: [],
      width: 2,
    },
  ];
  const labels: Label[] = [];
  // Stryker disable next-line ArrayDeclaration: a stray entry that is no box overlaps nothing.
  const placed: Box[] = [];
  for (const line of data.lines) {
    if (o.hidden.has(line.held)) continue;
    const style = styleOf(line.held);
    const points = line.segments.map((s) => s.vertices.map(px));
    for (const p of points)
      strokes.push({ points: p, colour: o.colour(style.colour), dash: style.dash, width: style.width });
    // A line of constant quality is labelled at its cool end, where those lines fan out, or not at all: at the other
    // they all meet at the critical point. The others at their far end, else their near one.
    const shown = points.flat().filter(inside);
    const [first, last] = [shown[0], shown.at(-1)];
    const ends = (line.held === "Q" ? [first] : [last, first]).filter((e): e is Point => e !== undefined);
    const label = place(lineLabel(line.held, line.value, o.system), ends, o.plot, placed);
    if (label) labels.push(label);
  }
  const dot = critical && px(critical);
  const symbol = (k: Key) => (output(k) as Output).symbol;
  return {
    plot: o.plot,
    strokes,
    dots: dot ? [{ x: dot[0], y: dot[1], r: 4, colour: ink }] : [],
    labels,
    xTicks: ticks(o.x, d.x.log, o.plot.left, o.plot.right, (text) => text.length * CHAR + 12),
    yTicks: ticks(o.y, d.y.log, o.plot.top, o.plot.bottom, () => 18),
    titles: { x: `${symbol(d.x.key)} / ${ux.label}`, y: `${symbol(d.y.key)} / ${uy.label}` },
  };
}

const round = (v: number) => Math.round(v * 10) / 10;

/** An SVG path through points, to a tenth of a pixel. */
export function pathOf(points: readonly (readonly [number, number])[]): string {
  return points.map(([x, y], i) => `${i === 0 ? "M" : "L"}${round(x)},${round(y)}`).join("");
}

const escape = (text: string) => text.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");

export interface SvgOptions {
  width: number;
  height: number;
  background: string;
  ink: string;
  muted: string;
  font: string;
}

/** The model as a standalone SVG document, its lines clipped to the plot. */
export function renderSvg(m: Model, o: SvgOptions): string {
  const { left, top, right, bottom } = m.plot;
  const box = `x="${left}" y="${top}" width="${right - left}" height="${bottom - top}"`;
  const paths = m.strokes.map(
    (s) =>
      `<path d="${pathOf(s.points)}" fill="none" stroke="${s.colour}" stroke-width="${s.width}"${
        s.dash.length > 0 ? ` stroke-dasharray="${s.dash.join(" ")}"` : ""
      }/>`,
  );
  const dots = m.dots.map((d) => `<circle cx="${round(d.x)}" cy="${round(d.y)}" r="${d.r}" fill="${d.colour}"/>`);
  const labels = m.labels.map(
    (l) =>
      `<text x="${round(l.x)}" y="${round(l.y)}" text-anchor="${l.anchor}" fill="${o.muted}">${escape(l.text)}</text>`,
  );
  const xTicks = m.xTicks.map(
    (t) =>
      `<text x="${round(t.at)}" y="${bottom + 18}" text-anchor="middle" fill="${o.muted}">${escape(t.text)}</text>`,
  );
  const yTicks = m.yTicks.map(
    (t) => `<text x="${left - 8}" y="${round(t.at) + 4}" text-anchor="end" fill="${o.muted}">${escape(t.text)}</text>`,
  );
  return [
    `<svg xmlns="http://www.w3.org/2000/svg" width="${o.width}" height="${o.height}" viewBox="0 0 ${o.width} ${o.height}" font-family="${escape(o.font)}" font-size="12">`,
    `<defs><clipPath id="plot"><rect ${box}/></clipPath></defs>`,
    `<rect width="${o.width}" height="${o.height}" fill="${o.background}"/>`,
    `<rect ${box} fill="none" stroke="${o.muted}"/>`,
    `<g clip-path="url(#plot)">${[...paths, ...dots, ...labels].join("")}</g>`,
    ...xTicks,
    ...yTicks,
    `<text x="${(left + right) / 2}" y="${o.height - 8}" text-anchor="middle" fill="${o.ink}">${escape(m.titles.x)}</text>`,
    `<text transform="translate(16 ${(top + bottom) / 2}) rotate(-90)" text-anchor="middle" fill="${o.ink}">${escape(m.titles.y)}</text>`,
    "</svg>",
  ].join("\n");
}

/** Every vertex in SI, one row each: the dome's liquid and vapour sides and its critical point, then each line. */
export function toCsv(d: Diagram, data: DiagramData): string {
  const rows = [`line,value,segment,vertex,${d.x.key},${d.y.key}`];
  const add = (line: string, value: string, segment: number, vertices: readonly Vertex[]) =>
    vertices.forEach((v, i) => rows.push(`${line},${value},${segment},${i},${v.x},${v.y}`));
  add("liquid", "", 0, data.dome.liquid);
  add("vapour", "", 0, data.dome.vapour);
  add("critical", "", 0, data.dome.critical ? [data.dome.critical] : []);
  for (const line of data.lines) line.segments.forEach((s, i) => add(line.held, String(line.value), i, s.vertices));
  return `${rows.join("\n")}\n`;
}

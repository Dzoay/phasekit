// A diagram's plot area and scales, in the shown units (PLAN.md M21.2), and the inverse the cursor's readout needs: a
// pixel back to the SI inputs of the pair its flash takes.
import { scaleLinear, scaleLog, type ScaleContinuousNumeric } from "d3-scale";
import { fromSI, toSI, type System } from "../lib/units";
import type { Axis, Diagram, Key } from "./planes";
import type { DiagramData } from "./data";
import { unitOfKey } from "./style";

export type Scale = ScaleContinuousNumeric<number, number>;

export interface Plot {
  left: number;
  top: number;
  right: number;
  bottom: number;
}

/** Room for the axes' ticks and titles. */
export const MARGIN = { left: 72, top: 16, right: 16, bottom: 48 } as const;

/** How far an axis reaches past its data: a fraction of its span, or of its decades on a log axis. */
export const PAD = 0.04;

export function padded([a, b]: readonly [number, number], log: boolean): [number, number] {
  if (log) {
    const factor = (b / a) ** PAD;
    return [a / factor, b * factor];
  }
  const pad = (b - a) * PAD;
  return [a - pad, b + pad];
}

function scale(axis: Axis, range: readonly [number, number], px: [number, number], system: System): Scale {
  const unit = unitOfKey(axis.key, system);
  const domain = padded([fromSI(range[0], unit), fromSI(range[1], unit)], axis.log);
  return (axis.log ? scaleLog() : scaleLinear()).domain(domain).range(px);
}

/** The plot area of a `width` × `height` diagram, and its scales from the data's ranges in the shown units. */
export function baseScales(
  d: Diagram,
  data: DiagramData,
  width: number,
  height: number,
  system: System,
): { x: Scale; y: Scale; plot: Plot } {
  const plot = { left: MARGIN.left, top: MARGIN.top, right: width - MARGIN.right, bottom: height - MARGIN.bottom };
  return {
    x: scale(d.x, data.x, [plot.left, plot.right], system),
    y: scale(d.y, data.y, [plot.bottom, plot.top], system),
    plot,
  };
}

/** The flash a point names, given in the shown units: the diagram's readout pair, its values in SI. */
export function readoutAt(d: Diagram, xv: number, yv: number, system: System): [Key, number, Key, number] {
  const at = new Map<Key, number>([
    [d.x.key, toSI(xv, unitOfKey(d.x.key, system))],
    [d.y.key, toSI(yv, unitOfKey(d.y.key, system))],
  ]);
  const [k1, k2] = d.readout;
  return [k1, at.get(k1) as number, k2, at.get(k2) as number];
}

/** The flash a pixel names: the diagram's readout pair, its values in SI. */
export function readoutInputs(
  d: Diagram,
  x: Scale,
  y: Scale,
  px: number,
  py: number,
  system: System,
): [Key, number, Key, number] {
  return readoutAt(d, x.invert(px), y.invert(py), system);
}

/** Where a readout's inputs sit on the diagram, in the shown units: [x, y]. */
export function shownAt(d: Diagram, inputs: readonly [Key, number, Key, number], system: System): [number, number] {
  const si = new Map<Key, number>([
    [inputs[0], inputs[1]],
    [inputs[2], inputs[3]],
  ]);
  return [
    fromSI(si.get(d.x.key) as number, unitOfKey(d.x.key, system)),
    fromSI(si.get(d.y.key) as number, unitOfKey(d.y.key, system)),
  ];
}

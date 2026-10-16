// A diagram's plot area and scales, in the shown units (PLAN.md M21.2), and the inverse the cursor's readout needs: a
// pixel back to the SI inputs of the pair its flash takes.
import { scaleLinear, scaleLog, type ScaleContinuousNumeric } from "d3-scale";
import { fromSI, toSI, type System } from "../lib/units";
import type { Axis, Diagram, Key } from "../sample/diagrams";
import type { DiagramData } from "../sample/lines";
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

/** The flash a pixel names: the diagram's readout pair, its values in SI. */
export function readoutInputs(
  d: Diagram,
  x: Scale,
  y: Scale,
  px: number,
  py: number,
  system: System,
): [Key, number, Key, number] {
  const at = new Map<Key, number>([
    [d.x.key, toSI(x.invert(px), unitOfKey(d.x.key, system))],
    [d.y.key, toSI(y.invert(py), unitOfKey(d.y.key, system))],
  ]);
  const [k1, k2] = d.readout;
  return [k1, at.get(k1) as number, k2, at.get(k2) as number];
}

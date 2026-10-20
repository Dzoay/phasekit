// A mixture's diagram (PLAN.md M21.5): its phase envelope on p–T or T–ρ, as the package traced it (`Fluid.envelope()`,
// M13.12), each vertex a bubble or a dew point of the feed, and the critical point the trace passed, drawn where the
// model puts it (Props1SI's, M13.7's). It adds no numerics: every vertex is the package's. The dome's liquid side is
// the bubble points, its vapour side the dew points, each running toward the critical point as a pure fluid's does.
import type { EnvelopeValue } from "../engine/protocol";
import type { Diagram, DiagramId } from "./diagrams";
import type { DiagramData, Vertex } from "./lines";

/** The constants a mixture's diagram reads, in this order: the model's critical point. */
export const MIXTURE_KEYS = ["T_critical", "p_critical", "rhomass_critical"] as const;

/** The diagrams a mixture has: the planes of its envelope. */
export const MIXTURE_DIAGRAMS: readonly DiagramId[] = ["pt", "trho"];

/** A plotted quantity's values, by the package's key. */
type Values = Record<"T" | "P" | "Dmass", number>;

/** A range around [lo, hi]: a logarithmic axis's widened by a factor of 2 each way, a linear axis's by a tenth of its
 *  span (of its value, where the span is 0). */
export function around(lo: number, hi: number, log: boolean): [number, number] {
  if (log) return [lo / 2, hi * 2];
  const pad = (hi - lo || Math.abs(hi)) / 10;
  return [lo - pad, hi + pad];
}

/** The envelope's vertices on `d`'s plane, its critical point among them where the trace passed it and `constants`
 *  (MIXTURE_KEYS' values) give it. Refused where the trace found no point, in the package's words. */
export function sampleEnvelope(d: Diagram, e: EnvelopeValue, constants: readonly (number | null)[]): DiagramData {
  if (e.T.length === 0) throw new Error(`the package traced no point of the envelope: ${e.reason}`);
  const plotted = (v: Values, inputs: Vertex["inputs"]): Vertex => ({
    x: v[d.x.key as keyof Values],
    y: v[d.y.key as keyof Values],
    inputs,
  });
  const at = (k: number): Vertex => {
    const [T, P, Dmass, Q] = [e.T[k], e.P[k], e.Dmass[k], e.Q[k]] as number[];
    return plotted({ T, P, Dmass } as Values, ["Q", Q as number, "T", T as number]);
  };
  const points = Array.from(e.T, (_, k) => k);
  // The side the trace starts on runs toward the critical point in the trace's order; the other runs away from it.
  const side = (q: number) => {
    const ks = points.filter((k) => e.Q[k] === q);
    return (ks[0] === 0 ? ks : ks.reverse()).map(at);
  };
  const [Tc, pc, rhoc] = constants;
  const known = e.critical !== null && [Tc, pc, rhoc].every((c) => Number.isFinite(c));
  const critical = known
    ? plotted({ T: Tc, P: pc, Dmass: rhoc } as Values, ["T", Tc as number, "Dmass", rhoc as number])
    : null;
  const dome = { liquid: side(0), vapour: side(1), critical };
  const all = [...dome.liquid, ...dome.vapour, ...(critical ? [critical] : [])];
  const span = (values: number[], log: boolean) => around(Math.min(...values), Math.max(...values), log);
  return {
    dome,
    lines: [],
    envelope: { components: e.components, feed: e.feed, end: e.end, reason: e.reason },
    x: span(
      all.map((v) => v.x),
      d.x.log,
    ),
    y: span(
      all.map((v) => v.y),
      d.y.log,
    ),
  };
}

// An engine whose answers the test releases, in any order, and minimal answers of the package's shapes, for the views'
// tests of late and superseded answers (PLAN.md M26.6).
import type {
  Body,
  CurvesValue,
  DiagramValue,
  Engine,
  MapValue,
  TernaryValue,
  Values,
} from "../../src/engine/protocol";

export interface Pending {
  body: Body;
  resolve: (value: unknown) => void;
  reject: (error: unknown) => void;
}

/** An engine that holds every call until the test answers it; `mixtures` it answers at once with none. */
export function heldEngine(): { engine: Engine; pending: Pending[] } {
  const pending: Pending[] = [];
  const engine: Engine = {
    call<B extends Body>(body: B): Promise<Values[B["op"]]> {
      if (body.op === "mixtures") return Promise.resolve({ predefined: [], pairs: [] } as unknown as Values[B["op"]]);
      return new Promise((resolve, reject) =>
        pending.push({ body, resolve: resolve as (value: unknown) => void, reject }),
      );
    },
  };
  return { engine, pending };
}

const noVertices = () => ({ xy: new Float64Array(), inputs: new Float64Array(), pairs: new Uint8Array() });

/** Curves of one curve, named `name`, without lines. */
export function curvesOf(name: string): CurvesValue {
  const runs = { starts: new Uint32Array([0]), T: new Float64Array(), rho: new Float64Array(), p: new Float64Array() };
  const empty = { ...runs, Z: new Float64Array(), inputs: new Float64Array(), pairs: new Uint8Array() };
  return {
    names: [name],
    zeroDensity: new Float64Array([NaN]),
    curves: new Uint32Array([0, 0]),
    lines: empty,
    isotherms: new Float64Array(),
    chart: empty,
    pairKeys: [],
  };
}

/** A diagram without a line or a dome. */
export function emptyDiagram(): DiagramValue {
  return {
    x: new Float64Array([0, 1]),
    y: new Float64Array([0, 1]),
    liquid: noVertices(),
    vapour: noVertices(),
    critical: noVertices(),
    lines: {
      held: [],
      values: new Float64Array(),
      segments: new Uint32Array([0]),
      twoPhase: new Uint8Array(),
      starts: new Uint32Array([0]),
      vertices: noVertices(),
    },
    pairKeys: [],
    mixture: null,
  };
}

/** A map of `key` without a level. */
export function mapOf(key: string): MapValue {
  return {
    plane: "ph",
    key,
    values: new Float64Array([0, 1]),
    x: new Float64Array([0, 1]),
    y: new Float64Array([0, 1]),
    levels: new Float64Array(),
    contours: new Uint32Array([0]),
    lines: { starts: new Uint32Array([0]), vertices: noVertices() },
    low: new Float64Array([NaN]),
    high: new Float64Array([NaN]),
    bands: new Uint32Array([0, 0]),
    polygons: { starts: new Uint32Array([0]), vertices: noVertices() },
    pairKeys: [],
  };
}

/** A ternary diagram at T without a point. */
export function ternaryAt(T: number): TernaryValue {
  return {
    components: ["A", "B", "C"],
    T,
    p: 1e5,
    lines: new Uint32Array([0]),
    binodal: new Float64Array(),
    binodalBeta: new Float64Array(),
    outside: new Float64Array(),
    feeds: new Float64Array(),
    beta: new Float64Array(),
    liquid: new Float64Array(),
    vapour: new Float64Array(),
    polygons: new Uint32Array([0]),
    region: new Float64Array(),
    feed: [],
  };
}

/** Let the answers released so far reach the view. */
export const drain = () => new Promise((r) => setTimeout(r, 0));

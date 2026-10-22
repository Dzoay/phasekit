// The messages between the page and its engine worker. Every call carries an id; its answer echoes the id and the op,
// with the value or the package's error message.

export type Build = "simd128" | "baseline";

export interface Info {
  build: Build;
  outputs: string[];
}

export interface StateValue {
  /** CoolProp's `PhaseSI` name of the state's phase. */
  phase: string;
  /** Output k's value, or null where the package refused it. */
  values: (number | null)[];
  /** Output k's refusal in the package's words, or null. */
  errors: (string | null)[];
  /** A mixture's composition and, at a two-phase state, its phases (PLAN.md M21.5); null for a pure fluid. */
  mixture: StateMixture | null;
}

/** A mixture at a state: the package's `Fluid.components` and `composition`, and its `State.phases()`. */
export interface StateMixture {
  /** The components' canonical names. */
  components: string[];
  /** The feed's mole fractions, one per component. */
  feed: number[];
  /** The liquid's and the vapour's mole fractions at a two-phase state; null in one phase, or where the package
   *  refused them. */
  liquid: number[] | null;
  vapour: number[] | null;
  /** Why the package refused the phases, in its words; null where it did not. */
  error: string | null;
}

/** What can be mixed (PLAN.md M21.5): the package's `predefinedMixtures()`, each with its `mixtureComponents`, and its
 *  `binaryPairs()`. */
export interface MixturesValue {
  /** Each predefined mixture's name (`R410A.mix`) and its components as the library lists them (`R32`, `NITROGEN`). */
  predefined: { name: string; components: string[] }[];
  /** The binary pairs, each two CAS numbers sorted as strings and joined by `&`. */
  pairs: string[];
}

/** A list of vertices, as the package's `Vertices` (PLAN.md M19.6): each vertex's x and y, two a vertex; the input
 *  values it was computed at, two a vertex; and its input pair, an index into the package's `pairKeys()`' pairs. */
export interface VerticesValue {
  xy: Float64Array;
  inputs: Float64Array;
  pairs: Uint8Array;
}

/** A diagram's isolines, as the package's `Lines`: line i holds `held[i]` at `values[i]` and runs in the segments from
 *  `segments[i]` to `segments[i + 1]`; segment j is two-phase where `twoPhase[j]` is 1, its vertices those from
 *  `starts[j]` to `starts[j + 1]`. */
export interface LinesValue {
  held: string[];
  values: Float64Array;
  segments: Uint32Array;
  twoPhase: Uint8Array;
  starts: Uint32Array;
  vertices: VerticesValue;
}

/** A fluid's diagram on one plane (the package's `Fluid.diagram`, PLAN.md M19.6): a pure fluid's dome and isolines, a
 *  mixture's phase envelope, as the package's typed arrays. */
export interface DiagramValue {
  /** The axes' spans, [lo, hi] each. */
  x: Float64Array;
  y: Float64Array;
  liquid: VerticesValue;
  vapour: VerticesValue;
  /** One vertex, or none. */
  critical: VerticesValue;
  lines: LinesValue;
  /** Each input pair's two keys, pair after pair (the package's `pairKeys()`). */
  pairKeys: string[];
  /** A mixture's components and the feed's mole fractions, how the trace of its envelope ended and why; null for a pure
   *  fluid. */
  mixture: { components: string[]; feed: number[]; end: string; reason: string } | null;
}

/** Runs of corners, as the package's `Corners`: run i the corners from `starts[i]` to `starts[i + 1]`, each corner's T,
 *  P, Hmass, Smass and Dmass five values of `corners`. */
export interface CornersValue {
  starts: Uint32Array;
  corners: Float64Array;
}

/** A fluid's phase regions (the package's `Fluid.regions`, PLAN.md M19.6). */
export interface RegionsValue {
  /** Each region's phase, as `PhaseSI` names it, and its outline, a run a region. */
  phases: string[];
  outlines: CornersValue;
  isotherm: CornersValue;
  isobar: CornersValue;
  /** The critical point's five values, or none. */
  critical: Float64Array;
  Tc: number;
  pc: number;
}

/**
 * A fluid's p–v–T and u–s–v surface (the package's `Fluid.surface`, PLAN.md M26.2): each vertex's state, T, ρ, p, u,
 * s and Q (NaN where it is not two-phase), its phase as `phaseNames[phases[i]]`; the triangles, three vertex indices
 * each; and the dome's vertex indices.
 */
export interface SurfaceValue {
  T: Float64Array;
  rho: Float64Array;
  p: Float64Array;
  u: Float64Array;
  s: Float64Array;
  Q: Float64Array;
  phases: Uint8Array;
  phaseNames: string[];
  triangles: Uint32Array;
  liquid: Uint32Array;
  vapour: Uint32Array;
  /** The critical point's vertex, or none. */
  critical: Uint32Array;
  /** The triple line's vertices, where the plotted range starts at the triple point. */
  triple: Uint32Array;
}

/** The mean time of one run, over `runs` runs repeated for a short budget. */
export interface Timing {
  us: number;
  runs: number;
}

export interface TimeValue {
  /** One flash at the inputs, a state handle made and freed, as the page calls it. */
  flash: Timing;
  /** Every output requested, read from one state. */
  outputs: Timing;
}

/** The benchmark's answer (PLAN.md M21.3): the batch's mean time over repeated runs, and its last answer. */
export interface BenchValue {
  timing: Timing;
  batch: BatchValue;
}

/** A batch's answer (the package's `Fluid.evaluate`): output k of point i at `values[i * outputs + k]`, its cell code
 *  at `status[i * outputs + k]` (0 success, 1 a flagged extrapolation, more a failure). */
export interface BatchValue {
  values: Float64Array;
  status: Uint8Array;
  outputs: number;
}

/** What the engine's registry serves and has decoded (the package's `names()` and `loaded()`, PLAN.md M21.4). */
export interface RegistryValue {
  /** The canonical name of every fluid it serves, the newest pack's first and the five embedded last. */
  names: string[];
  /** The fluids it has decoded so far, in the order of its layers. */
  loaded: string[];
}

export type Request =
  | { id: number; op: "info" }
  | { id: number; op: "registry" }
  | { id: number; op: "pack"; bytes: Uint8Array }
  | {
      id: number;
      op: "props";
      output: string;
      name1: string;
      value1: number;
      name2: string;
      value2: number;
      fluid: string;
    }
  | {
      id: number;
      op: "state";
      fluid: string;
      name1: string;
      value1: number;
      name2: string;
      value2: number;
      outputs: string[];
    }
  | { id: number; op: "constants"; fluid: string; keys: string[] }
  | { id: number; op: "mixtures" }
  // A name, not a fluid: the engine answers it before any pack is added (src/engine/client.ts).
  | { id: number; op: "components"; name: string }
  | { id: number; op: "diagram"; fluid: string; plane: string }
  | { id: number; op: "regions"; fluid: string }
  | { id: number; op: "surface"; fluid: string }
  | {
      id: number;
      op: "batch";
      fluid: string;
      outputs: string[];
      name1: string;
      values1: Float64Array;
      name2: string;
      values2: Float64Array;
    }
  | {
      id: number;
      op: "bench";
      fluid: string;
      outputs: string[];
      name1: string;
      values1: Float64Array;
      name2: string;
      values2: Float64Array;
    }
  | {
      id: number;
      op: "time";
      fluid: string;
      name1: string;
      value1: number;
      name2: string;
      value2: number;
      outputs: string[];
    };

export type Op = Request["op"];

export interface Values {
  info: Info;
  registry: RegistryValue;
  pack: RegistryValue;
  props: number;
  state: StateValue;
  time: TimeValue;
  /** Each key's `Props1SI` value, or null where the package refused it. */
  constants: (number | null)[];
  mixtures: MixturesValue;
  /** The fluids a mixture name needs (the package's `mixtureComponents`); none for a pure fluid's name. */
  components: string[];
  diagram: DiagramValue;
  regions: RegionsValue;
  surface: SurfaceValue;
  batch: BatchValue;
  bench: BenchValue;
}

export type Response =
  | { [K in Op]: { id: number; ok: true; op: K; value: Values[K] } }[Op]
  | { id: number; ok: false; op: Op; error: string };

/** A request without its id, which the engine's client assigns. */
export type Body = { [K in Op]: Omit<Extract<Request, { op: K }>, "id"> }[Op];

/** What the page needs of an engine: the worker's client, or in tests the handler itself. */
export interface Engine {
  call<B extends Body>(body: B): Promise<Values[B["op"]]>;
}

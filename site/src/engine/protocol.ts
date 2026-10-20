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

/** A mixture's phase envelope (the package's `Fluid.envelope()`, PLAN.md M21.5): one entry a point, in the trace's order,
 *  each a bubble point (Q = 0) or a dew point (Q = 1) of the feed. */
export interface EnvelopeValue {
  T: Float64Array;
  P: Float64Array;
  Q: Float64Array;
  /** The feed's mass density. */
  Dmass: Float64Array;
  /** The incipient phase's mole fractions, point-major: component i's at point k at `k * components.length + i`. */
  incipient: Float64Array;
  /** The components' canonical names, and the feed's mole fractions. */
  components: string[];
  feed: number[];
  /** The critical point the trace passed, (T, p) where every ln K is 0; null where it passed none. */
  critical: [number, number] | null;
  /** How the trace ended: `closed`, `bound` or `failed`; and why, in the package's words, where it did not close. */
  end: string;
  reason: string;
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
  | { id: number; op: "envelope"; fluid: string }
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
  envelope: EnvelopeValue;
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

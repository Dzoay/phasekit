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

/** A batch's answer (the package's `Fluid.evaluate`): output k of point i at `values[i * outputs + k]`, its cell code
 *  at `status[i * outputs + k]` (0 success, 1 a flagged extrapolation, more a failure). */
export interface BatchValue {
  values: Float64Array;
  status: Uint8Array;
  outputs: number;
}

export type Request =
  | { id: number; op: "info" }
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
  props: number;
  state: StateValue;
  time: TimeValue;
  /** Each key's `Props1SI` value, or null where the package refused it. */
  constants: (number | null)[];
  batch: BatchValue;
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

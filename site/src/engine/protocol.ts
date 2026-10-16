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
    };

export type Op = Request["op"];

export interface Values {
  info: Info;
  props: number;
  state: StateValue;
}

export type Response =
  | { [K in Op]: { id: number; ok: true; op: K; value: Values[K] } }[Op]
  | { id: number; ok: false; op: Op; error: string };

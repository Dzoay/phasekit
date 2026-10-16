// The engine: the page's requests (protocol.ts) answered by the browser package. It adds no numerics: every value is
// the package's, and every refusal is the package's message. It holds one registry and the fluid handles it has
// opened; each state is freed once its outputs are read.
import type { Pk } from "./pk";
import type { BatchValue, Build, Request, Response, StateValue, TimeValue, Timing } from "./protocol";

// The handle types; wasm-bindgen's private constructors rule out `InstanceType`.
type Registry = Pk["Registry"]["prototype"];
type Fluid = Pk["Fluid"]["prototype"];

const message = (e: unknown): string => (e instanceof Error ? e.message : String(e));

/** How long a timing repeats its run: long enough that a browser's coarse clock (100 µs without isolation) blurs the
 *  mean by under 1 %. */
export const BUDGET_MS = 20;

/** The most runs a timing makes, whatever the clock says: a clock that never moves cannot hang the worker. */
export const MAX_RUNS = 1_000_000;

/** The mean time of `run` in µs, repeated for the budget after one untimed run that warms it (and throws if it fails).
 *  `clock` gives milliseconds. */
export function measure(run: () => void, clock: () => number = () => performance.now()): Timing {
  run();
  const start = clock();
  let [runs, now] = [0, start];
  do {
    run();
    runs += 1;
    now = clock();
  } while (now - start < BUDGET_MS && runs < MAX_RUNS);
  return { us: ((now - start) * 1000) / runs, runs };
}

export function createHandler(pk: Pk, build: Build): (request: Request) => Response {
  let registry: Registry | undefined;
  const fluids = new Map<string, Fluid>();

  const fluid = (name: string): Fluid => {
    let handle = fluids.get(name);
    if (!handle) {
      registry ??= new pk.Registry();
      handle = registry.fluid(name);
      fluids.set(name, handle);
    }
    return handle;
  };

  const state = (r: Extract<Request, { op: "state" }>): StateValue => {
    const s = fluid(r.fluid).flash(r.name1, r.value1, r.name2, r.value2);
    try {
      const values: (number | null)[] = [];
      const errors: (string | null)[] = [];
      for (const key of r.outputs) {
        try {
          values.push(s.get(key));
          errors.push(null);
        } catch (e) {
          values.push(null);
          errors.push(message(e));
        }
      }
      return { phase: s.phase, values, errors };
    } finally {
      s.free();
    }
  };

  // A flash with a state handle made and freed, as the page makes one; then every output read from one state.
  const time = (r: Extract<Request, { op: "time" }>): TimeValue => {
    const f = fluid(r.fluid);
    const flash = measure(() => f.flash(r.name1, r.value1, r.name2, r.value2).free());
    const s = f.flash(r.name1, r.value1, r.name2, r.value2);
    try {
      const outputs = measure(() => {
        for (const key of r.outputs) {
          try {
            s.get(key);
          } catch {
            // A refused output costs its refusal, as on the page.
          }
        }
      });
      return { flash, outputs };
    } finally {
      s.free();
    }
  };

  // Each key's Props1SI value, null where the package refuses it; an unknown fluid refuses the whole request.
  const constants = (r: Extract<Request, { op: "constants" }>): (number | null)[] => {
    fluid(r.fluid);
    return r.keys.map((key) => {
      try {
        return (registry as Registry).Props1SI(key, r.fluid);
      } catch {
        return null;
      }
    });
  };

  // The package's batch, its handle freed once its arrays are copied out.
  const batch = (r: Extract<Request, { op: "batch" }>): BatchValue => {
    const b = fluid(r.fluid).evaluate(r.outputs, r.name1, r.values1, r.name2, r.values2);
    try {
      return { values: b.values, status: b.status, outputs: b.outputs };
    } finally {
      b.free();
    }
  };

  return (request) => {
    const { id, op } = request;
    try {
      switch (request.op) {
        case "info":
          return { id, ok: true, op: "info", value: { build, outputs: pk.outputKeys() } };
        case "props": {
          registry ??= new pk.Registry();
          const { output, name1, value1, name2, value2 } = request;
          return {
            id,
            ok: true,
            op: "props",
            value: registry.PropsSI(output, name1, value1, name2, value2, request.fluid),
          };
        }
        case "state":
          return { id, ok: true, op: "state", value: state(request) };
        case "time":
          return { id, ok: true, op: "time", value: time(request) };
        case "constants":
          return { id, ok: true, op: "constants", value: constants(request) };
        case "batch":
          return { id, ok: true, op: "batch", value: batch(request) };
      }
    } catch (e) {
      return { id, ok: false, op, error: message(e) };
    }
  };
}

// The engine: the page's requests (protocol.ts) answered by the browser package. It adds no numerics: every value is
// the package's, and every refusal is the package's message. It holds one registry, which each pack the page adds
// replaces with a registry that also serves the pack's fluids (PLAN.md M21.4), and the fluid handles it has opened,
// which outlive the registry they came from; each state, set of phases, diagram and set of regions is freed once it is
// read, and so is each handle read from it.
import type { Pk } from "./pk";
import type {
  BatchValue,
  BenchValue,
  Build,
  CornersValue,
  DiagramValue,
  MixturesValue,
  RegionsValue,
  RegistryValue,
  Request,
  Response,
  StateMixture,
  StateValue,
  TimeValue,
  Timing,
  VerticesValue,
} from "./protocol";

// The handle types; wasm-bindgen's private constructors rule out `InstanceType`.
type Registry = Pk["Registry"]["prototype"];
type Fluid = Pk["Fluid"]["prototype"];
type State = Pk["State"]["prototype"];

const message = (e: unknown): string => (e instanceof Error ? e.message : String(e));

/** What `read` takes from a handle, the handle freed once it is read. */
function freed<H extends { free(): void }, T>(handle: H, read: (handle: H) => T): T {
  try {
    return read(handle);
  } finally {
    handle.free();
  }
}

const vertices = (v: { xy: Float64Array; inputs: Float64Array; pairs: Uint8Array; free(): void }): VerticesValue =>
  freed(v, (h) => ({ xy: h.xy, inputs: h.inputs, pairs: h.pairs }));

const corners = (c: { starts: Uint32Array; corners: Float64Array; free(): void }): CornersValue =>
  freed(c, (h) => ({ starts: h.starts, corners: h.corners }));

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

  const reg = (): Registry => (registry ??= new pk.Registry());

  const fluid = (name: string): Fluid => {
    let handle = fluids.get(name);
    if (!handle) {
      handle = reg().fluid(name);
      fluids.set(name, handle);
    }
    return handle;
  };

  const contents = (): RegistryValue => ({ names: reg().names(), loaded: reg().loaded() });

  // A pack's fluids over the registry's: the new registry replaces the old, which is freed; a refused pack (corrupt, or
  // naming a fluid the registry has) leaves the registry as it was.
  const pack = (bytes: Uint8Array): RegistryValue => {
    const next = reg().withPack(bytes);
    reg().free();
    registry = next;
    return contents();
  };

  // A mixture's composition, and its phases at a two-phase state (PLAN.md M21.5); null for a pure fluid, the one
  // component of its own. A refusal of the phases is kept in the package's words, and the state's values stand.
  const mixtureAt = (f: Fluid, s: State): StateMixture | null => {
    const components = f.components;
    if (components.length < 2) return null;
    const at: StateMixture = { components, feed: [...f.composition], liquid: null, vapour: null, error: null };
    try {
      const phases = s.phases();
      if (!phases) return at;
      try {
        return { ...at, liquid: [...phases.liquid], vapour: [...phases.vapour] };
      } finally {
        phases.free();
      }
    } catch (e) {
      return { ...at, error: message(e) };
    }
  };

  const state = (r: Extract<Request, { op: "state" }>): StateValue => {
    const f = fluid(r.fluid);
    const s = f.flash(r.name1, r.value1, r.name2, r.value2);
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
      return { phase: s.phase, values, errors, mixture: mixtureAt(f, s) };
    } finally {
      s.free();
    }
  };

  // What can be mixed: the predefined mixtures with their components, and the binary pairs.
  const mixtures = (): MixturesValue => ({
    predefined: pk.predefinedMixtures().map((name) => ({ name, components: pk.mixtureComponents(name) })),
    pairs: pk.binaryPairs(),
  });

  // A fluid's diagram on a plane (PLAN.md M21.6), its arrays copied out of the package's handles; a mixture's with its
  // components, its feed and how the trace of its envelope ended.
  const diagram = (name: string, plane: string): DiagramValue => {
    const f = fluid(name);
    return freed(f.diagram(plane), (d) => ({
      x: d.x,
      y: d.y,
      liquid: vertices(d.liquid),
      vapour: vertices(d.vapour),
      critical: vertices(d.critical),
      lines: freed(d.lines, (l) => ({
        held: l.held,
        values: l.values,
        segments: l.segments,
        twoPhase: l.twoPhase,
        starts: l.starts,
        vertices: vertices(l.vertices),
      })),
      pairKeys: pk.pairKeys(),
      mixture:
        f.components.length < 2
          ? null
          : { components: f.components, feed: [...f.composition], end: d.end, reason: d.reason },
    }));
  };

  // A fluid's phase regions (PLAN.md M21.6), their arrays copied out of the package's handles.
  const regions = (name: string): RegionsValue =>
    freed(fluid(name).regions(), (r) => ({
      phases: r.phases,
      outlines: corners(r.outlines),
      isotherm: corners(r.isotherm),
      isobar: corners(r.isobar),
      critical: r.critical,
      Tc: r.Tc,
      pc: r.pc,
    }));

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
        return reg().Props1SI(key, r.fluid);
      } catch {
        return null;
      }
    });
  };

  // The package's batch, its handle freed once its arrays are copied out.
  const batch = (r: Omit<Extract<Request, { op: "batch" }>, "id" | "op">): BatchValue => {
    const b = fluid(r.fluid).evaluate(r.outputs, r.name1, r.values1, r.name2, r.values2);
    try {
      return { values: b.values, status: b.status, outputs: b.outputs };
    } finally {
      b.free();
    }
  };

  // The benchmark (PLAN.md M21.3): the batch timed as the page calls it, a handle made and freed each run, then its
  // answer once more.
  const bench = (r: Extract<Request, { op: "bench" }>): BenchValue => {
    const f = fluid(r.fluid);
    const timing = measure(() => f.evaluate(r.outputs, r.name1, r.values1, r.name2, r.values2).free());
    return { timing, batch: batch(r) };
  };

  return (request) => {
    const { id, op } = request;
    try {
      switch (request.op) {
        case "info":
          return { id, ok: true, op: "info", value: { build, outputs: pk.outputKeys() } };
        case "registry":
          return { id, ok: true, op: "registry", value: contents() };
        case "pack":
          return { id, ok: true, op: "pack", value: pack(request.bytes) };
        case "props": {
          const { output, name1, value1, name2, value2 } = request;
          return {
            id,
            ok: true,
            op: "props",
            value: reg().PropsSI(output, name1, value1, name2, value2, request.fluid),
          };
        }
        case "state":
          return { id, ok: true, op: "state", value: state(request) };
        case "time":
          return { id, ok: true, op: "time", value: time(request) };
        case "constants":
          return { id, ok: true, op: "constants", value: constants(request) };
        case "mixtures":
          return { id, ok: true, op: "mixtures", value: mixtures() };
        case "components":
          return { id, ok: true, op: "components", value: pk.mixtureComponents(request.name) };
        case "diagram":
          return { id, ok: true, op: "diagram", value: diagram(request.fluid, request.plane) };
        case "regions":
          return { id, ok: true, op: "regions", value: regions(request.fluid) };
        case "batch":
          return { id, ok: true, op: "batch", value: batch(request) };
        case "bench":
          return { id, ok: true, op: "bench", value: bench(request) };
      }
    } catch (e) {
      return { id, ok: false, op, error: message(e) };
    }
  };
}

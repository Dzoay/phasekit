// The engine the page's worker runs, under Node on the real package: it adds no numerics, so what it answers is the
// package's value, bit for bit (`Exact`, VERIFICATION.md §5).
import { describe, expect, test, vi } from "vitest";
import { BUDGET_MS, MAX_RUNS, createHandler, measure } from "../../src/engine/handler";
import type { Pk } from "../../src/engine/pk";
import { simdSupported } from "../../src/engine/simd";
import { embedded, loadPk, packBytes, propsSI, sameBits } from "../pk";

describe("engine", () => {
  test("answers PropsSI with the package's value", async () => {
    const handle = createHandler(await loadPk("simd"), "simd128");
    const want = await propsSI("simd", "T", "P", 101325, "Q", 0, "Water");
    const got = handle({
      id: 7,
      op: "props",
      output: "T",
      name1: "P",
      value1: 101325,
      name2: "Q",
      value2: 0,
      fluid: "Water",
    });
    expect(got.id).toBe(7);
    expect(got.ok && typeof got.value === "number" && sameBits(got.value, want), JSON.stringify(got)).toBe(true);
  });

  test("lists a state's outputs, each value the package's and each refusal in its words", async () => {
    const pk = await loadPk("simd");
    const handle = createHandler(pk, "simd128");
    const outputs = ["Hmass", "Cpmass", "fraction_min"];
    const got = handle({
      id: 1,
      op: "state",
      fluid: "Water",
      name1: "T",
      value1: 300,
      name2: "P",
      value2: 1e5,
      outputs,
    });
    expect(got.ok, JSON.stringify(got)).toBe(true);
    if (!got.ok || got.op !== "state") return;
    const registry = new pk.Registry();
    const fluid = registry.fluid("Water");
    const state = fluid.flash("T", 300, "P", 1e5);
    expect(got.value.phase).toBe(state.phase);
    for (const [i, key] of outputs.entries()) {
      let want: number | string;
      try {
        want = state.get(key);
      } catch (e) {
        want = (e as Error).message;
      }
      const [value, error] = [got.value.values[i], got.value.errors[i]];
      if (typeof want === "number")
        expect(value !== null && value !== undefined && sameBits(value, want), key).toBe(true);
      else expect([value, error?.message], key).toEqual([null, want]);
    }
    state.free();
    fluid.free();
    registry.free();
  });

  test("refuses an unknown fluid in the package's words", async () => {
    const handle = createHandler(await loadPk("simd"), "simd128");
    const got = handle({
      id: 2,
      op: "state",
      fluid: "Unobtainium",
      name1: "T",
      value1: 300,
      name2: "P",
      value2: 1e5,
      outputs: ["T"],
    });
    expect(got.ok).toBe(false);
    expect(
      !got.ok && got.error.message.length > 0 && got.error.message.includes("Unobtainium"),
      JSON.stringify(got),
    ).toBe(true);
  });

  test("reports its build", async () => {
    const handle = createHandler(await loadPk("baseline"), "baseline");
    const got = handle({ id: 3, op: "info" });
    expect(got.ok && got.op === "info" && got.value.build).toBe("baseline");
  });

  test("Node runs simd128, so the probe accepts it", () => {
    expect(simdSupported()).toBe(true);
  });
});

describe("timing", () => {
  test("times a flash and its outputs, each the mean of many runs", async () => {
    const handle = createHandler(await loadPk("simd"), "simd128");
    const request = { fluid: "Water", name1: "P", value1: 101325, name2: "Q", value2: 0, outputs: ["T", "Hmass"] };
    const got = handle({ id: 4, op: "time", ...request });
    expect(got.ok && got.op === "time", JSON.stringify(got)).toBe(true);
    if (!got.ok || got.op !== "time") return;
    for (const timing of [got.value.flash, got.value.outputs]) {
      expect(timing.runs).toBeGreaterThanOrEqual(1);
      expect(Number.isFinite(timing.us) && timing.us > 0).toBe(true);
    }
  });

  test("times nothing where the package refuses the state", async () => {
    const handle = createHandler(await loadPk("simd"), "simd128");
    const got = handle({
      id: 5,
      op: "time",
      fluid: "Water",
      name1: "T",
      value1: 0,
      name2: "P",
      value2: 1e5,
      outputs: [],
    });
    expect(got.ok).toBe(false);
  });
});

describe("the SIMD probe", () => {
  test("is false where the browser rejects the probe or cannot validate", () => {
    const validate = vi.spyOn(WebAssembly, "validate");
    validate.mockReturnValueOnce(false);
    expect(simdSupported()).toBe(false);
    validate.mockImplementationOnce(() => {
      throw new Error("no WebAssembly");
    });
    expect(simdSupported()).toBe(false);
    validate.mockRestore();
  });
});

/** A package that counts what the engine does with it: its registries, fluid handles, flashes, reads and frees. */
function fakePk() {
  const calls = {
    registries: 0,
    registriesFreed: 0,
    fluids: 0,
    flashes: 0,
    reads: 0,
    freed: 0,
    evaluated: 0,
    batchesFreed: 0,
    phasesFreed: 0,
    /** Handles freed by kind: a diagram and its lines, vertices, regions and corners. */
    handlesFreed: {} as Record<string, number>,
  };
  class Handle {
    free(): void {
      const kind = this.constructor.name;
      calls.handlesFreed[kind] = (calls.handlesFreed[kind] ?? 0) + 1;
    }
  }
  class Vertices extends Handle {
    xy = new Float64Array([1, 2]);
    inputs = new Float64Array([0, 300]);
    pairs = new Uint8Array([0]);
  }
  class Lines extends Handle {
    held = ["T"];
    values = new Float64Array([300]);
    segments = new Uint32Array([0, 1]);
    twoPhase = new Uint8Array([0]);
    starts = new Uint32Array([0, 1]);
    get vertices(): Vertices {
      return new Vertices();
    }
  }
  class Diagram extends Handle {
    x = new Float64Array([0, 1]);
    y = new Float64Array([2, 3]);
    end = "closed";
    reason = "";
    get liquid(): Vertices {
      return new Vertices();
    }
    get vapour(): Vertices {
      return new Vertices();
    }
    get critical(): Vertices {
      return new Vertices();
    }
    get lines(): Lines {
      return new Lines();
    }
  }
  class Corners extends Handle {
    starts = new Uint32Array([0, 1]);
    corners = new Float64Array([1, 2, 3, 4, 5]);
  }
  class Regions extends Handle {
    phases = ["gas"];
    critical = new Float64Array([6, 7, 8, 9, 10]);
    Tc = 6;
    pc = 7;
    get outlines(): Corners {
      return new Corners();
    }
    get isotherm(): Corners {
      return new Corners();
    }
    get isobar(): Corners {
      return new Corners();
    }
  }
  class Phases {
    liquid = new Float64Array([0.75, 0.25]);
    vapour = new Float64Array([0.25, 0.75]);
    free(): void {
      calls.phasesFreed += 1;
    }
  }
  class State {
    phase = "liquid";
    #mixed: string;
    constructor(mixed: string) {
      this.#mixed = mixed;
    }
    get(key: string): number {
      calls.reads += 1;
      if (key === "refused") throw new Error("not here");
      if (key === "odd") throw "a bare string";
      return 1;
    }
    // A mixture's two-phase state has phases, its one-phase state none; the "Unsolved" mixture's refuses them.
    phases(): Phases | undefined {
      if (this.#mixed === "Unsolved") throw new Error("the vapour-liquid equilibrium solve (Vle) did not converge in 60 iterations");
      return this.#mixed === "Split" ? new Phases() : undefined;
    }
    free(): void {
      calls.freed += 1;
    }
  }
  class Batch {
    values = new Float64Array([7]);
    status = new Uint8Array([0]);
    outputs = 1;
    free(): void {
      calls.batchesFreed += 1;
    }
  }
  // A fluid named "Split", "Single" or "Unsolved" is a mixture of two components; any other a pure fluid.
  class Fluid {
    #name: string;
    constructor(name: string) {
      this.#name = name;
    }
    get components(): string[] {
      return ["Split", "Single", "Unsolved"].includes(this.#name) ? ["A", "B"] : [this.#name];
    }
    get composition(): Float64Array {
      return new Float64Array(this.components.length === 2 ? [0.5, 0.5] : [1]);
    }
    evaluate(): Batch {
      calls.evaluated += 1;
      return new Batch();
    }
    flash(): State {
      calls.flashes += 1;
      return new State(this.#name);
    }
    diagram(plane: string): Diagram {
      if (plane === "xy") throw new Error("`xy` is no plane");
      return new Diagram();
    }
    regions(): Regions {
      return new Regions();
    }
  }
  class Registry {
    constructor() {
      calls.registries += 1;
    }
    fluid(name: string): Fluid {
      calls.fluids += 1;
      if (name === "Nothing") throw new Error("unknown fluid Nothing");
      return new Fluid(name);
    }
    PropsSI(): number {
      return 42;
    }
    withPack(): Registry {
      return new Registry();
    }
    names(): string[] {
      return ["Water"];
    }
    loaded(): string[] {
      return [];
    }
    free(): void {
      calls.registriesFreed += 1;
    }
  }
  return { pk: { Registry, outputKeys: () => ["T"], pairKeys: () => ["Q", "T"] } as unknown as Pk, calls };
}

const at = { name1: "T", value1: 300, name2: "P", value2: 1e5 };

describe("the engine's handles", () => {
  test("opens one registry and each fluid once", () => {
    const { pk, calls } = fakePk();
    const handle = createHandler(pk, "baseline");
    handle({ id: 1, op: "state", fluid: "Water", ...at, outputs: ["T"] });
    handle({ id: 2, op: "state", fluid: "Water", ...at, outputs: ["T"] });
    const props = handle({ id: 3, op: "props", output: "T", ...at, fluid: "Water" });
    expect(props).toEqual({ id: 3, ok: true, op: "props", value: 42 });
    expect([calls.registries, calls.fluids]).toEqual([1, 1]);
  });

  test("frees every state it makes, a timing's included", () => {
    const { pk, calls } = fakePk();
    const handle = createHandler(pk, "baseline");
    const state = handle({ id: 1, op: "state", fluid: "Water", ...at, outputs: ["T", "refused", "odd"] });
    expect(state).toEqual({
      id: 1,
      ok: true,
      op: "state",
      value: {
        phase: "liquid",
        values: [1, null, null],
        errors: [null, { message: "not here", explained: {} }, { message: "a bare string", explained: {} }],
        mixture: null,
      },
    });
    expect([calls.flashes, calls.freed]).toEqual([1, 1]);
    const time = handle({ id: 2, op: "time", fluid: "Water", ...at, outputs: ["T", "refused"] });
    expect(time.ok && time.op).toBe("time");
    expect(calls.flashes).toBeGreaterThan(3);
    expect(calls.freed).toBe(calls.flashes);
    expect(calls.reads).toBeGreaterThan(3 + 4);
  });

  test("answers a refused fluid with the package's message", () => {
    const { pk } = fakePk();
    const got = createHandler(pk, "baseline")({ id: 9, op: "time", fluid: "Nothing", ...at, outputs: [] });
    expect(got).toEqual({ id: 9, ok: false, op: "time", error: { message: "unknown fluid Nothing", explained: {} } });
  });
});

describe("measure", () => {
  test("warms once, then repeats until the budget has passed", () => {
    let [t, calls] = [5, 0];
    const timing = measure(
      () => (calls += 1),
      () => (t += 1),
    );
    expect(timing).toEqual({ us: 1000, runs: BUDGET_MS });
    expect(calls).toBe(BUDGET_MS + 1);
  });

  // A million runs: quick, but under Stryker's instrumentation of the handler, on a busy machine, more than 5 s.
  test("stops at its cap when the clock does not move", { timeout: 60_000 }, () => {
    const timing = measure(
      () => {},
      () => 0,
    );
    expect(timing).toEqual({ us: 0, runs: MAX_RUNS });
  });
});

describe("the engine's constants and batches", () => {
  test("reads a fluid's constants as Props1SI does, and null for a key that is not one", async () => {
    const pk = await loadPk("simd");
    const handle = createHandler(pk, "simd128");
    const keys = ["T_critical", "p_critical", "T_triple", "T", "P_max"];
    const got = handle({ id: 1, op: "constants", fluid: "Water", keys });
    const registry = new pk.Registry();
    const want = keys.map((k) => {
      try {
        return registry.Props1SI(k, "Water");
      } catch {
        return null;
      }
    });
    registry.free();
    expect(want[3]).toBeNull();
    expect(got).toEqual({ id: 1, ok: true, op: "constants", value: want });
    expect(handle({ id: 2, op: "constants", fluid: "Unobtainium", keys }).ok).toBe(false);
  });

  test("runs a batch as the package's evaluate does, bit for bit, failed cells coded", async () => {
    const pk = await loadPk("simd");
    const handle = createHandler(pk, "simd128");
    const [values1, values2] = [new Float64Array([300, 500, -1]), new Float64Array([1e5, 1e6, 1e5])];
    const got = handle({
      id: 3,
      op: "batch",
      fluid: "Water",
      outputs: ["Hmass", "Q"],
      name1: "T",
      values1,
      name2: "P",
      values2,
    });
    expect(got.ok && got.op).toBe("batch");
    if (!got.ok || got.op !== "batch") return;
    const fluid = new pk.Registry().fluid("Water");
    const want = fluid.evaluate(["Hmass", "Q"], "T", values1, "P", values2);
    expect([...got.value.values].map((v) => Object.is(v, NaN) || v)).toEqual(
      [...want.values].map((v) => Object.is(v, NaN) || v),
    );
    expect([...got.value.status]).toEqual([...want.status]);
    expect(got.value.outputs).toBe(2);
    expect(got.value.status[4]).toBeGreaterThan(1);
    want.free();
    fluid.free();
  });

  test("frees each registry a pack replaces", () => {
    const { pk, calls } = fakePk();
    const handle = createHandler(pk, "baseline");
    for (const id of [1, 2]) expect(handle({ id, op: "pack", bytes: new Uint8Array([id]) }).ok).toBe(true);
    expect([calls.registries, calls.registriesFreed]).toEqual([3, 2]);
  });

  test("frees each batch it runs", () => {
    const { pk, calls } = fakePk();
    const handle = createHandler(pk, "baseline");
    const got = handle({
      id: 4,
      op: "batch",
      fluid: "Water",
      outputs: ["T"],
      name1: "T",
      values1: new Float64Array([1]),
      name2: "P",
      values2: new Float64Array([1]),
    });
    expect(got).toEqual({
      id: 4,
      ok: true,
      op: "batch",
      value: { values: new Float64Array([7]), status: new Uint8Array([0]), outputs: 1 },
    });
    expect([calls.evaluated, calls.batchesFreed]).toEqual([1, 1]);
  });
});

describe("the engine's packs (PLAN.md M21.4)", () => {
  const state = (fluid: string) => ({
    op: "state" as const,
    fluid,
    name1: "P",
    value1: 101325,
    name2: "Q",
    value2: 0,
    outputs: ["T"],
  });

  test("its registry names the five embedded fluids; a pack adds its own, first, and fluids opened before it still compute", async () => {
    const handle = createHandler(await loadPk("simd"), "simd128");
    const five = await embedded();
    expect(handle({ id: 1, op: "registry" })).toEqual({
      id: 1,
      ok: true,
      op: "registry",
      value: { names: five, loaded: [] },
    });
    const water = handle({ id: 2, ...state("Water") });
    expect(handle({ id: 3, op: "pack", bytes: packBytes("r32.pack") })).toEqual({
      id: 3,
      ok: true,
      op: "pack",
      value: { names: ["R32", ...five], loaded: ["Water"] },
    });
    expect(handle({ id: 4, ...state("Water") })).toEqual({ ...water, id: 4 });
    const r32 = handle({ id: 5, ...state("R32") });
    expect(r32.ok && r32.op === "state" && r32.value.values[0], JSON.stringify(r32)).toBeCloseTo(221.5, 0);
    expect(handle({ id: 6, op: "registry" })).toEqual({
      id: 6,
      ok: true,
      op: "registry",
      value: { names: ["R32", ...five], loaded: ["Water", "R32"] },
    });
  });

  test("a refused pack, corrupt or naming a fluid the registry serves, leaves the registry as it was", async () => {
    const handle = createHandler(await loadPk("simd"), "simd128");
    const corrupt = packBytes("r32.pack");
    corrupt.set([(corrupt.at(-1) as number) ^ 1], corrupt.length - 1);
    const refused = handle({ id: 1, op: "pack", bytes: corrupt });
    expect(refused).toEqual({
      id: 1,
      ok: false,
      op: "pack",
      error: expect.objectContaining({ message: expect.stringContaining("checksum") }),
    });
    const twice = handle({ id: 2, op: "pack", bytes: packBytes("water.pack") });
    expect(twice).toEqual({
      id: 2,
      ok: false,
      op: "pack",
      error: expect.objectContaining({ message: expect.stringContaining("already registered") }),
    });
    expect(handle({ id: 3, op: "registry" })).toEqual({
      id: 3,
      ok: true,
      op: "registry",
      value: { names: await embedded(), loaded: [] },
    });
    expect(handle({ id: 4, ...state("Water") }).ok).toBe(true);
  });
});

describe("the engine's mixtures (PLAN.md M21.5)", () => {
  /** The engine on the real package with R32's and R125's packs: R410A.mix's components. */
  async function r410a() {
    const pk = await loadPk("simd");
    const handle = createHandler(pk, "simd128");
    for (const [id, file] of [
      [1, "r32.pack"],
      [2, "r125.pack"],
    ] as const)
      expect(handle({ id, op: "pack", bytes: packBytes(file) }).ok).toBe(true);
    let registry = new pk.Registry();
    for (const file of ["r32.pack", "r125.pack"]) {
      const next = registry.withPack(packBytes(file));
      registry.free();
      registry = next;
    }
    return { pk, handle, registry };
  }

  test("a mixture's state has its composition and, two-phase, each phase's: the package's, bit for bit", async () => {
    const { handle, registry } = await r410a();
    const fluid = registry.fluid("R410A.mix");
    const split = fluid.flash("Q", 0.5, "T", 280);
    const phases = split.phases();
    expect(phases).toBeDefined();
    const got = handle({
      id: 3,
      op: "state",
      fluid: "R410A.mix",
      name1: "Q",
      value1: 0.5,
      name2: "T",
      value2: 280,
      outputs: ["P"],
    });
    expect(got.ok && got.op === "state" && got.value.mixture).toEqual({
      components: ["R32", "R125"],
      feed: [...fluid.composition],
      liquid: [...(phases?.liquid ?? [])],
      vapour: [...(phases?.vapour ?? [])],
      error: null,
    });
    const gas = handle({ id: 4, op: "state", fluid: "R410A.mix", ...at, outputs: ["P"] });
    expect(gas.ok && gas.op === "state" && gas.value.mixture).toEqual({
      components: ["R32", "R125"],
      feed: [...fluid.composition],
      liquid: null,
      vapour: null,
      error: null,
    });
    const water = handle({
      id: 5,
      op: "state",
      fluid: "Water",
      name1: "Q",
      value1: 0.5,
      name2: "T",
      value2: 373,
      outputs: [],
    });
    expect(water.ok && water.op === "state" && water.value.mixture).toBeNull();
    for (const handle of [phases, split, fluid, registry]) handle?.free();
  });

  test("lists the predefined mixtures with their components, and the binary pairs, as the package does", async () => {
    const pk = await loadPk("simd");
    const got = createHandler(pk, "simd128")({ id: 1, op: "mixtures" });
    const predefined = pk.predefinedMixtures().map((name) => ({ name, components: pk.mixtureComponents(name) }));
    expect(got).toEqual({ id: 1, ok: true, op: "mixtures", value: { predefined, pairs: pk.binaryPairs() } });
    expect(predefined).toContainEqual({ name: "R410A.mix", components: ["R32", "R125"] });
  });

  test("names a mixture's components before its packs are added; none for a pure fluid; a malformed name refused", async () => {
    const handle = createHandler(await loadPk("simd"), "simd128");
    expect(handle({ id: 1, op: "components", name: "R410A.mix" })).toEqual({
      id: 1,
      ok: true,
      op: "components",
      value: ["R32", "R125"],
    });
    expect(handle({ id: 2, op: "components", name: "Water" })).toEqual({
      id: 2,
      ok: true,
      op: "components",
      value: [],
    });
    expect(handle({ id: 3, op: "components", name: "R999.mix" })).toEqual({
      id: 3,
      ok: false,
      op: "components",
      error: expect.objectContaining({ message: expect.stringContaining("R999.mix") }),
    });
  });

  test("draws a mixture's envelope as the package does, bit for bit, with its components, feed and end", async () => {
    const { pk, handle, registry } = await r410a();
    const fluid = registry.fluid("R410A.mix");
    const got = handle({ id: 3, op: "diagram", fluid: "R410A.mix", plane: "trho" });
    const want = fluid.diagram("trho");
    expect(got.ok && got.op === "diagram" && got.value).toEqual({
      ...arrays(want),
      pairKeys: pk.pairKeys(),
      mixture: { components: ["R32", "R125"], feed: [...fluid.composition], end: "closed", reason: "" },
    });
    expect(handle({ id: 4, op: "diagram", fluid: "R410A.mix", plane: "ph" })).toEqual({
      id: 4,
      ok: false,
      op: "diagram",
      error: expect.objectContaining({ message: "input pair HP is not supported by this model" }),
    });
    expect(handle({ id: 5, op: "regions", fluid: "R410A.mix" })).toEqual({
      id: 5,
      ok: false,
      op: "regions",
      error: { message: `${fluid.name} is a mixture: its phase regions are not drawn`, explained: {} },
    });
    for (const h of [want, fluid, registry]) h.free();
  });

  test("frees each state's phases, diagram and set of regions; keeps a refusal of the phases in the package's words", () => {
    const { pk, calls } = fakePk();
    const handle = createHandler(pk, "baseline");
    const split = handle({ id: 1, op: "state", fluid: "Split", ...at, outputs: [] });
    expect(split.ok && split.op === "state" && split.value.mixture).toEqual({
      components: ["A", "B"],
      feed: [0.5, 0.5],
      liquid: [0.75, 0.25],
      vapour: [0.25, 0.75],
      error: null,
    });
    const single = handle({ id: 2, op: "state", fluid: "Single", ...at, outputs: [] });
    expect(single.ok && single.op === "state" && single.value.mixture?.liquid).toBeNull();
    const unsolved = handle({ id: 3, op: "state", fluid: "Unsolved", ...at, outputs: ["T"] });
    expect(unsolved).toEqual({
      id: 3,
      ok: true,
      op: "state",
      value: {
        phase: "liquid",
        values: [1],
        errors: [null],
        mixture: {
          components: ["A", "B"],
          feed: [0.5, 0.5],
          liquid: null,
          vapour: null,
          error: "the vapour-liquid equilibrium solve (Vle) did not converge in 60 iterations",
        },
      },
    });
    expect(calls.phasesFreed).toBe(1);
    const diagram = handle({ id: 4, op: "diagram", fluid: "Split", plane: "pt" });
    expect(diagram.ok && diagram.op === "diagram" && diagram.value.mixture).toEqual({
      components: ["A", "B"],
      feed: [0.5, 0.5],
      end: "closed",
      reason: "",
    });
    // The diagram, its lines, and its liquid, vapour, critical point and lines' vertices.
    expect(calls.handlesFreed).toEqual({ Diagram: 1, Lines: 1, Vertices: 4 });
  });
});

describe("the engine's diagrams (PLAN.md M21.6)", () => {
  test("draws a pure fluid's diagram and regions as the package does, bit for bit", async () => {
    const pk = await loadPk("simd");
    const handle = createHandler(pk, "simd128");
    const registry = new pk.Registry();
    const water = registry.fluid("Water");
    const got = handle({ id: 1, op: "diagram", fluid: "Water", plane: "ts" });
    const want = water.diagram("ts");
    expect(got.ok && got.op === "diagram" && got.value).toEqual({
      ...arrays(want),
      pairKeys: pk.pairKeys(),
      mixture: null,
    });
    const regions = handle({ id: 2, op: "regions", fluid: "Water" });
    const r = water.regions();
    const corners = (c: { starts: Uint32Array; corners: Float64Array; free(): void }) => {
      const value = { starts: c.starts, corners: c.corners };
      c.free();
      return value;
    };
    expect(regions.ok && regions.op === "regions" && regions.value).toEqual({
      phases: r.phases,
      outlines: corners(r.outlines),
      isotherm: corners(r.isotherm),
      isobar: corners(r.isobar),
      critical: r.critical,
      Tc: r.Tc,
      pc: r.pc,
    });
    expect(handle({ id: 3, op: "diagram", fluid: "Water", plane: "xy" })).toEqual({
      id: 3,
      ok: false,
      op: "diagram",
      error: expect.objectContaining({ message: "`xy` is no plane: ph, ts, pt, trho or hs" }),
    });
    for (const h of [want, r, water, registry]) h.free();
  });

  test("frees each diagram and set of regions, and every handle read from them", () => {
    const { pk, calls } = fakePk();
    const handle = createHandler(pk, "baseline");
    const vertices = { xy: new Float64Array([1, 2]), inputs: new Float64Array([0, 300]), pairs: new Uint8Array([0]) };
    expect(handle({ id: 1, op: "diagram", fluid: "Water", plane: "ph" })).toEqual({
      id: 1,
      ok: true,
      op: "diagram",
      value: {
        x: new Float64Array([0, 1]),
        y: new Float64Array([2, 3]),
        liquid: vertices,
        vapour: vertices,
        critical: vertices,
        lines: {
          held: ["T"],
          values: new Float64Array([300]),
          segments: new Uint32Array([0, 1]),
          twoPhase: new Uint8Array([0]),
          starts: new Uint32Array([0, 1]),
          vertices,
        },
        pairKeys: ["Q", "T"],
        mixture: null,
      },
    });
    expect(calls.handlesFreed).toEqual({ Diagram: 1, Lines: 1, Vertices: 4 });
    const corners = { starts: new Uint32Array([0, 1]), corners: new Float64Array([1, 2, 3, 4, 5]) };
    expect(handle({ id: 2, op: "regions", fluid: "Water" })).toEqual({
      id: 2,
      ok: true,
      op: "regions",
      value: {
        phases: ["gas"],
        outlines: corners,
        isotherm: corners,
        isobar: corners,
        critical: new Float64Array([6, 7, 8, 9, 10]),
        Tc: 6,
        pc: 7,
      },
    });
    expect(calls.handlesFreed).toEqual({ Diagram: 1, Lines: 1, Vertices: 4, Regions: 1, Corners: 3 });
    expect(handle({ id: 3, op: "diagram", fluid: "Water", plane: "xy" })).toEqual({
      id: 3,
      ok: false,
      op: "diagram",
      error: expect.objectContaining({ message: "`xy` is no plane" }),
    });
  });
});

type Vertices = { xy: Float64Array; inputs: Float64Array; pairs: Uint8Array; free(): void };

/** A diagram's arrays as the package gives them, each handle read freed. */
function arrays(d: {
  x: Float64Array;
  y: Float64Array;
  liquid: Vertices;
  vapour: Vertices;
  critical: Vertices;
  lines: {
    held: string[];
    values: Float64Array;
    segments: Uint32Array;
    twoPhase: Uint8Array;
    starts: Uint32Array;
    vertices: Vertices;
    free(): void;
  };
}) {
  const vertices = (v: Vertices) => {
    const value = { xy: v.xy, inputs: v.inputs, pairs: v.pairs };
    v.free();
    return value;
  };
  const l = d.lines;
  const lines = {
    held: l.held,
    values: l.values,
    segments: l.segments,
    twoPhase: l.twoPhase,
    starts: l.starts,
    vertices: vertices(l.vertices),
  };
  l.free();
  return {
    x: d.x,
    y: d.y,
    liquid: vertices(d.liquid),
    vapour: vertices(d.vapour),
    critical: vertices(d.critical),
    lines,
  };
}

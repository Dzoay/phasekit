// The page's fluid data (PLAN.md M21.4): a fluid's packs are fetched on its first use, each fluid it requires first,
// once each, none for a fluid the registry serves; a failure refuses the call and is tried again; and the page says
// where each fluid's data comes from and what it has fetched and decoded. A mixture's packs are its components', which
// the engine names (PLAN.md M21.5), and the engine lists what can be mixed. Then the same on the real package, every
// pack of packs/index.json, and R410A.mix's two.
import { describe, expect, test } from "vitest";
import { createHandler } from "../../src/engine/handler";
import type { Request, Response } from "../../src/engine/protocol";
import { FluidData, type Registry, type Source } from "../../src/lib/fluiddata.svelte";
import type { PackEntry } from "../../src/lib/packs";
import { embedded, loadPk, packBytes, packIndex } from "../pk";

const entry = (name: string, bytes: number, requires: string[] = [], cas: string | null = null): PackEntry => ({
  name,
  aliases: [],
  cas,
  requires,
  pack: `${name.toLowerCase()}.pack`,
  bytes,
});

const water = entry("Water", 70_000);
const c = entry("C", 456);
const b = entry("B", 2_000, ["C", "Water"]);
const a = entry("A", 1_234, ["B"], "1-11-1");
const d = entry("D", 5_000, ["B"], "2-22-2");
const INDEX = [a, b, c, d, water];

/** What the fake engine mixes: A with D, by their CAS numbers; AD.mix the one predefined mixture it builds, and AB.mix
 *  one it lists but cannot build (B has no CAS number). */
const MIXABLE = {
  predefined: [
    { name: "AB.mix", components: ["A", "B"] },
    { name: "AD.mix", components: ["a", "D"] },
  ],
  pairs: ["1-11-1&2-22-2"],
};

/** A source and registry that log what they are asked, the registry serving Water to begin with. */
function fakes() {
  const log: string[] = [];
  const state = {
    names: ["Water"],
    loaded: [] as string[],
    failing: new Set<string>(),
    indexFailures: 0,
    catalogueFailures: 0,
  };
  const source: Source = {
    async index() {
      log.push("index");
      if (state.indexFailures > 0) {
        state.indexFailures -= 1;
        throw new Error("offline");
      }
      return { dataset: "test", fluids: INDEX };
    },
    async pack(file) {
      log.push(`fetch ${file}`);
      if (state.failing.has(file)) throw new Error(`${file}: 404 Not Found`);
      return new TextEncoder().encode(file);
    },
  };
  const registry: Registry = {
    async add(bytes) {
      const file = new TextDecoder().decode(bytes);
      log.push(`add ${file}`);
      state.names = [(INDEX.find((f) => f.pack === file) as PackEntry).name, ...state.names];
      return { names: state.names, loaded: state.loaded };
    },
    async contents() {
      log.push("contents");
      return { names: state.names, loaded: state.loaded };
    },
    // AD.mix and A[0.5]&D[0.5] are mixtures of A and D; Bad.mix is refused; any other name names none.
    async components(name) {
      log.push(`components ${name}`);
      if (name === "Bad.mix") throw new Error("unknown name Bad.mix");
      return name === "AD.mix" || name === "A[0.5]&D[0.5]" ? ["a", "D"] : [];
    },
    async mixtures() {
      log.push("mixtures");
      if (state.catalogueFailures > 0) {
        state.catalogueFailures -= 1;
        throw new Error("the engine stopped");
      }
      return MIXABLE;
    },
  };
  return { data: new FluidData(source, registry), log, state };
}

/** Lets every settled promise's callbacks run. */
const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

describe("a fluid's packs", () => {
  test("are none for a fluid the registry serves, in any letter case, and the index is not fetched", async () => {
    const { data, log } = fakes();
    await data.ensure("Water");
    await data.ensure("WATER");
    expect(log).toEqual(["contents"]);
    expect([data.fetched, data.names]).toEqual([[], ["Water"]]);
  });

  test("come in order, each fluid it requires first, each once, then no more", async () => {
    const { data, log } = fakes();
    await data.ensure("A");
    expect(log).toEqual([
      "contents",
      "index",
      "fetch c.pack",
      "add c.pack",
      "fetch b.pack",
      "add b.pack",
      "fetch a.pack",
      "add a.pack",
    ]);
    expect(data.fetched).toEqual([c, b, a]);
    expect([data.bytes, data.names]).toEqual([3_690, ["A", "B", "C", "Water"]]);
    await data.ensure("a");
    await data.ensure("D");
    expect(log.slice(8)).toEqual(["fetch d.pack", "add d.pack"]);
  });

  test("asked for by two fluids at once are fetched once", async () => {
    const { data, log } = fakes();
    await Promise.all([data.ensure("A"), data.ensure("D")]);
    expect(log.filter((l) => l.startsWith("fetch"))).toEqual([
      "fetch c.pack",
      "fetch b.pack",
      "fetch a.pack",
      "fetch d.pack",
    ]);
    expect(data.fetched.map((f) => f.name)).toEqual(["C", "B", "A", "D"]);
  });

  test("that cannot be fetched refuse the call, and are tried again", async () => {
    const { data, log, state } = fakes();
    state.failing.add("b.pack");
    await expect(data.ensure("A")).rejects.toThrow("b.pack: 404 Not Found");
    expect(data.fetched).toEqual([c]);
    state.failing.clear();
    await data.ensure("A");
    expect(log.filter((l) => l.startsWith("fetch"))).toEqual([
      "fetch c.pack",
      "fetch b.pack",
      "fetch b.pack",
      "fetch a.pack",
    ]);
    expect(data.fetched).toEqual([c, b, a]);
  });

  test("are none for a name the index lacks, or while the index cannot be fetched, which is said and tried again", async () => {
    const { data, log } = fakes();
    await data.ensure("Unobtainium");
    expect(log).toEqual(["contents", "index", "components Unobtainium"]);
    const other = new FluidData(
      { index: () => Promise.reject(new Error("offline")), pack: () => Promise.reject(new Error("no")) },
      registryOf(),
    );
    await other.ensure("A");
    expect([other.error, other.fluids]).toEqual(["the fluid index could not be fetched: offline", []]);
    const fresh = fakes();
    fresh.state.indexFailures = 1;
    await expect(fresh.data.index()).rejects.toThrow("offline");
    expect(fresh.data.error).toBe("the fluid index could not be fetched: offline");
    expect(await fresh.data.index()).toBe(INDEX);
    expect([fresh.data.error, fresh.data.fluids]).toEqual([null, INDEX]);
    expect(fresh.log).toEqual(["index", "index"]);
  });

  test("the index is fetched once", async () => {
    const { data, log } = fakes();
    await Promise.all([data.index(), data.index()]);
    await data.index();
    expect(log).toEqual(["index"]);
  });
});

describe("a mixture's packs (PLAN.md M21.5)", () => {
  test("are its components', in order, each fluid they require first; the engine names them once a name", async () => {
    const { data, log } = fakes();
    await data.ensure("AD.mix");
    expect(log).toEqual([
      "contents",
      "index",
      "components AD.mix",
      "fetch c.pack",
      "add c.pack",
      "fetch b.pack",
      "add b.pack",
      "fetch a.pack",
      "add a.pack",
      "fetch d.pack",
      "add d.pack",
    ]);
    expect(data.parts).toEqual(new Map([["AD.mix", ["a", "D"]]]));
    await data.ensure("AD.mix");
    expect(log.length).toBe(11);
    await data.ensure("A[0.5]&D[0.5]");
    expect(log.slice(11)).toEqual(["components A[0.5]&D[0.5]"]);
    expect([...data.parts.keys()]).toEqual(["AD.mix", "A[0.5]&D[0.5]"]);
  });

  test("are none for a name the engine refuses or names no component of, and it is asked again", async () => {
    const { data, log } = fakes();
    await data.ensure("Bad.mix");
    await data.ensure("Bad.mix");
    expect(log).toEqual(["contents", "index", "components Bad.mix", "components Bad.mix"]);
    expect(data.parts.size).toBe(0);
  });

  test("the predefined mixtures it builds, from the engine's list and the index, asked once", async () => {
    const { data, log } = fakes();
    expect([data.mixable, data.pairs.size, data.mixtures]).toEqual([{ predefined: [], pairs: [] }, 0, []]);
    await Promise.all([data.catalogue(), data.catalogue()]);
    expect(data.mixtures).toEqual([]);
    await data.index();
    expect(data.mixtures).toEqual([{ name: "AD.mix", components: [a, d] }]);
    await data.catalogue();
    expect(log).toEqual(["mixtures", "index"]);
  });

  test("a list the engine refuses is asked for again", async () => {
    const { data, log, state } = fakes();
    state.catalogueFailures = 1;
    await expect(data.catalogue()).rejects.toThrow("the engine stopped");
    await data.catalogue();
    expect(data.mixable).toBe(MIXABLE);
    expect(log).toEqual(["mixtures", "mixtures"]);
  });

  test("the page says where each component's data comes from", async () => {
    const { data } = fakes();
    await data.ensure("AD.mix");
    expect(data.origin("AD.mix")).toBe("AD.mix: a mixture of A, fetched, 1.23 kB; D, fetched, 5.00 kB.");
    data.parts = new Map([["AX.mix", ["A", "X"]]]);
    expect(data.origin("AX.mix")).toBe("AX.mix: a mixture of A, fetched, 1.23 kB; X.");
  });
});

describe("the registry", () => {
  test("is asked once on one touch", async () => {
    const { data, log } = fakes();
    data.touch();
    await settle();
    await settle();
    expect(log).toEqual(["contents"]);
  });

  test("is asked again on a touch, and once more after a request in flight, never twice at once", async () => {
    const { data, log, state } = fakes();
    data.touch();
    data.touch();
    data.touch();
    expect(log).toEqual(["contents"]);
    state.loaded = ["Water"];
    await settle();
    await settle();
    expect(log).toEqual(["contents", "contents"]);
    expect(data.loaded).toEqual(["Water"]);
    data.touch();
    await settle();
    expect(log.length).toBe(3);
  });

  test("that cannot be reached leaves a touch quiet", async () => {
    const data = new FluidData(
      { index: async () => ({ dataset: "", fluids: [] }), pack: () => Promise.reject(new Error("no")) },
      { ...registryOf(), contents: () => Promise.reject(new Error("the engine stopped")) },
    );
    data.touch();
    await settle();
    expect(data.names).toEqual([]);
    await expect(data.ensure("Water")).rejects.toThrow("the engine stopped");
  });
});

describe("the page says", () => {
  test("where a fluid's data comes from, and each fluid's it requires", async () => {
    const { data } = fakes();
    expect([data.origin("Water"), data.origin("A")]).toEqual(["", ""]);
    await data.refresh();
    expect(data.origin("Water")).toBe("Water: embedded in the package.");
    await data.index();
    expect(data.origin("water")).toBe("Water: embedded in the package.");
    expect(data.origin("Unobtainium")).toBe("");
    expect(data.origin("B")).toBe(
      "B: 2.00 kB to fetch; its transport reads C, 456 B to fetch; Water, embedded in the package.",
    );
    await data.ensure("A");
    expect(data.origin("A")).toBe("A: fetched, 1.23 kB; its transport reads B, fetched, 2.00 kB.");
    expect([data.cost(water), data.cost(a), data.cost(d)]).toEqual(["embedded", "fetched", "5.00 kB"]);
    const unlisted = new FluidData(sourceOf([entry("X", 9, ["Gone"])]), registryOf());
    await unlisted.index();
    expect(unlisted.origin("X")).toBe("X: 9 B to fetch.");
  });

  test("what it has fetched and decoded", async () => {
    const { data, state } = fakes();
    expect(data.summary).toBe("This page has fetched 0 packs, 0 B of 0 B, and decoded no fluid.");
    await data.index();
    await data.ensure("C");
    expect(data.summary).toBe("This page has fetched 1 pack, 456 B of 78.7 kB, and decoded no fluid.");
    state.loaded = ["Water", "C"];
    await data.ensure("B");
    expect(data.summary).toBe("This page has fetched 2 packs, 2.46 kB of 78.7 kB, and decoded Water, C.");
  });
});

function sourceOf(fluids: PackEntry[]): Source {
  return { index: async () => ({ dataset: "", fluids }), pack: () => Promise.reject(new Error("no")) };
}

function registryOf(): Registry {
  return {
    add: () => Promise.reject(new Error("no")),
    contents: async () => ({ names: [], loaded: [] }),
    components: async () => [],
    mixtures: async () => ({ predefined: [], pairs: [] }),
  };
}

describe("on the real package", () => {
  test(
    "R32 comes from its pack, its reference embedded, and every fluid of the index from its packs, by any of its names",
    { timeout: 120_000 },
    async () => {
      const handle = createHandler(await loadPk("simd"), "simd128");
      let id = 0;
      const call = (body: Record<string, unknown>): Response => handle({ ...body, id: ++id } as Request);
      const answer = (r: Response) =>
        r.ok && (r.op === "pack" || r.op === "registry") ? r.value : Promise.reject(new Error(r.ok ? r.op : r.error));
      const data = new FluidData(
        { index: async () => packIndex(), pack: async (file) => packBytes(file) },
        {
          add: async (bytes) => answer(call({ op: "pack", bytes })),
          contents: async () => answer(call({ op: "registry" })),
          components: async () => [],
          mixtures: async () => ({ predefined: [], pairs: [] }),
        },
      );
      await data.ensure("75-10-5");
      expect(data.fetched.map((f) => f.pack)).toEqual(["r32.pack"]);
      expect(data.names).toEqual(["R32", ...(await embedded())]);
      const state = call({
        op: "state",
        fluid: "R32",
        name1: "T",
        value1: 300,
        name2: "Dmolar",
        value2: 100,
        outputs: ["L"],
      });
      expect(state.ok, JSON.stringify(state)).toBe(true);
      await data.refresh();
      expect(data.loaded).toEqual(["n-Propane", "R32"]);

      const index = packIndex().fluids;
      for (const f of index) await data.ensure(f.name);
      expect([data.fetched.length, data.names.length, data.bytes]).toEqual([
        131,
        136,
        index.filter((f) => !EMBEDDED.includes(f.name)).reduce((sum, f) => sum + f.bytes, 0),
      ]);
      const failures: string[] = [];
      for (const f of index) {
        const tc = (fluid: string) => {
          const r = call({ op: "constants", fluid, keys: ["T_critical"] });
          return r.ok && r.op === "constants" ? r.value[0] : `${fluid}: ${r.ok ? r.op : r.error}`;
        };
        const want = tc(f.name);
        if (typeof want !== "number") failures.push(String(want));
        for (const name of [...(f.cas === null ? [] : [f.cas]), ...f.aliases])
          if (tc(name) !== want) failures.push(`${f.name} by ${name}: ${tc(name)}`);
      }
      expect(failures).toEqual([]);
    },
  );

  test("R410A.mix comes from R32's and R125's packs, which the engine names", async () => {
    const handle = createHandler(await loadPk("simd"), "simd128");
    let id = 0;
    const call = (body: Record<string, unknown>): Response => handle({ ...body, id: ++id } as Request);
    const value = <T>(r: Response): T =>
      r.ok
        ? (r.value as T)
        : (() => {
            throw new Error(r.error);
          })();
    const data = new FluidData(
      { index: async () => packIndex(), pack: async (file) => packBytes(file) },
      {
        add: async (bytes) => value(call({ op: "pack", bytes })),
        contents: async () => value(call({ op: "registry" })),
        components: async (name) => value(call({ op: "components", name })),
        mixtures: async () => value(call({ op: "mixtures" })),
      },
    );
    await data.ensure("R410A.mix");
    expect(data.fetched.map((f) => f.pack)).toEqual(["r32.pack", "r125.pack"]);
    const state = call({
      op: "state",
      fluid: "R410A.mix",
      name1: "T",
      value1: 300,
      name2: "P",
      value2: 1e5,
      outputs: [],
    });
    expect(state.ok, JSON.stringify(state)).toBe(true);
    await data.catalogue();
    expect(data.mixtures.map((m) => m.name)).toContain("R410A.mix");
  });
});

const EMBEDDED = await embedded();

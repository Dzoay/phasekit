// The diagrams page's view (PLAN.md M21.2), with its wiring, under Node on the real package: the route loads the view,
// the view writes the URL, a fluid and diagram load their geometry once, from one call and no batch (M21.6), and the
// cursor's readout is the package's flash at the pixel's inputs.
import { flushSync } from "svelte";
import { describe, expect, test, vi } from "vitest";
import type { Key } from "../../src/diagram/planes";
import { DiagramView, READOUT_KEYS } from "../../src/diagram/view.svelte";
import { createHandler } from "../../src/engine/handler";
import type { Body, Engine, Request, Values } from "../../src/engine/protocol";
import { loadPk, packBytes } from "../pk";

async function realEngine(): Promise<{ engine: Engine; calls: string[]; bodies: Body[] }> {
  const handle = createHandler(await loadPk("simd"), "simd128");
  const calls: string[] = [];
  const bodies: Body[] = [];
  let id = 0;
  const engine: Engine = {
    call<B extends Body>(body: B): Promise<Values[B["op"]]> {
      calls.push(body.op);
      bodies.push(body);
      const r = handle({ ...body, id: ++id } as Request);
      return r.ok
        ? Promise.resolve(r.value as Values[B["op"]])
        : Promise.reject(Object.assign(new Error(r.error.message), { refusal: r.error }));
    },
  };
  return { engine, calls, bodies };
}

async function setup(query: string) {
  const route = $state({ params: new URLSearchParams(query) });
  const writes: string[] = [];
  const { engine, calls } = await realEngine();
  const view = new DiagramView(engine);
  const stop = view.connect(route, (p) => writes.push(p.toString()));
  await settle(view);
  return { view, route, writes, calls, stop };
}

const settle = (view: DiagramView) =>
  vi.waitFor(
    () => {
      flushSync();
      expect(view.busy).toBe(false);
    },
    { timeout: 20_000 },
  );

const last = (writes: string[]) => writes.at(-1) ?? "";

// Sampling a real diagram takes a second or two, beside the other test files.
describe("the diagrams view", { timeout: 30_000 }, () => {
  test("loads its view from the route and samples the diagram", async () => {
    const { view, writes, stop } = await setup("fluid=Water&d=ts");
    expect([view.fluid, view.id, view.system, [...view.hidden]]).toEqual(["Water", "ts", "si", []]);
    expect(view.diagram.title).toBe("T–s");
    expect(view.data?.lines.length).toBeGreaterThan(10);
    expect(view.refusal).toBeNull();
    expect(last(writes)).toBe("fluid=Water&d=ts");
    stop();
  });

  test("a bad route falls back to Water's p–h diagram in SI", async () => {
    const { view, stop } = await setup("d=nonsense&units=furlongs&hide=X,T");
    expect([view.fluid, view.id, view.system, [...view.hidden]]).toEqual(["Water", "ph", "si", ["T"]]);
    stop();
  });

  test("toggling a family and the units writes the URL and keeps the geometry", async () => {
    const { view, writes, calls, stop } = await setup("fluid=Water&d=ph");
    const data = view.data;
    const batches = calls.filter((c) => c === "batch").length;
    view.toggle("T");
    view.toggle("Q");
    view.setSystem("engineering");
    flushSync();
    expect(last(writes)).toBe("fluid=Water&d=ph&units=engineering&hide=T%2CQ");
    view.toggle("T");
    flushSync();
    expect(last(writes)).toBe("fluid=Water&d=ph&units=engineering&hide=Q");
    expect(view.data).toBe(data);
    expect(calls.filter((c) => c === "batch").length).toBe(batches);
    stop();
  });

  test("a new fluid or diagram samples anew; an earlier one comes from the cache", async () => {
    const { view, calls, stop } = await setup("fluid=Water&d=ph");
    const water = view.data;
    view.choose("pt");
    await settle(view);
    expect(view.data?.lines.every((l) => l.held === "Dmass")).toBe(true);
    view.fluid = "Nitrogen";
    await settle(view);
    expect(view.data).not.toBe(water);
    const before = calls.length;
    view.fluid = "Water";
    view.choose("ph");
    await settle(view);
    expect(view.data).toBe(water);
    expect(calls.length).toBe(before);
    stop();
  });

  test("an unknown fluid is refused in the package's words", async () => {
    const { view, stop } = await setup("fluid=Unobtainium&d=ph");
    expect(view.data).toBeNull();
    expect(view.refusal).toContain("Unobtainium");
    stop();
  });

  test("the readout is the package's state at the pixel's inputs; a later point wins", async () => {
    const { view, stop } = await setup("fluid=Water&d=ph");
    view.point(["Hmass", 2.8e6, "P", 1e5]);
    view.point(["Hmass", 400e3, "P", 1e6]);
    await vi.waitFor(() => expect(view.readout?.state).toBeTruthy());
    expect(view.readout?.inputs).toEqual(["Hmass", 400e3, "P", 1e6]);
    expect(view.readout?.state?.phase).toBe("liquid");
    expect(view.readout?.state?.values.length).toBe(READOUT_KEYS.length);
    view.point(["Hmass", -1e9, "P", 1e6]);
    await vi.waitFor(() => expect(view.readout?.refusal).toBeTruthy());
    expect(view.readout?.state).toBeNull();
    view.leave();
    expect(view.readout).toBeNull();
    stop();
  });

  test("disconnecting stops the wiring", async () => {
    const { view, route, writes, stop } = await setup("fluid=Water&d=ph");
    stop();
    const count = writes.length;
    route.params = new URLSearchParams("fluid=R134a&d=ts");
    view.toggle("T");
    flushSync();
    expect([view.fluid, writes.length]).toEqual(["Water", count]);
  });
});

/** The real engine, but each call of `ops` waits until the test answers it, in any order. */
async function held(ops: string[]) {
  const { engine: real } = await realEngine();
  const waiting: { body: Body; resolve: (v: unknown) => void; reject: (e: unknown) => void }[] = [];
  const engine: Engine = {
    call: <B extends Body>(body: B) =>
      ops.includes(body.op)
        ? new Promise<Values[B["op"]]>((resolve, reject) =>
            waiting.push({ body, resolve: resolve as (v: unknown) => void, reject }),
          )
        : real.call(body),
  };
  const answer = (i: number) => {
    const w = waiting[i] as (typeof waiting)[number];
    return real.call(w.body).then(w.resolve, w.reject);
  };
  return { engine, waiting, answer };
}

const tick = () => new Promise((r) => setTimeout(r, 0));

describe("the diagrams view, before and between answers", { timeout: 30_000 }, () => {
  test("a new view shows Water's p–h diagram in SI and is busy until its first answer", async () => {
    const { engine } = await realEngine();
    const view = new DiagramView(engine);
    expect([view.fluid, view.id, view.system, [...view.hidden], view.busy, view.data, view.readout]).toEqual([
      "Water",
      "ph",
      "si",
      [],
      true,
      null,
      null,
    ]);
  });

  test("loads the engineer's units and families with long names from the URL", async () => {
    const { view, stop } = await setup("fluid=Water&d=ts&units=engineering&hide=Smass,Q");
    expect([view.system, [...view.hidden]]).toEqual(["engineering", ["Smass", "Q"]]);
    stop();
  });

  // PLAN.md M21.6: the package computes the geometry, so the page asks it once and samples nothing itself.
  test("draws a diagram from one engine call and no batch, and the regions from one more", async () => {
    const { view, calls, stop } = await setup("fluid=Water&d=ts");
    expect(calls).toEqual(["diagram"]);
    expect(view.data?.dome.liquid.length).toBeGreaterThan(100);
    view.toggleRegions();
    await vi.waitFor(() => {
      flushSync();
      expect(view.phases).not.toBeNull();
    });
    expect(calls).toEqual(["diagram", "regions"]);
    stop();
  });

  test("keeps one readout flash in flight, then asks for the latest point only", async () => {
    const { engine, waiting, answer } = await held(["state"]);
    const view = new DiagramView(engine);
    const at = (p: number): [Key, number, Key, number] => ["Hmass", 400e3, "P", p];
    const asked = () => waiting.map((w) => ("value2" in w.body ? w.body.value2 : null));
    view.point(at(1e5));
    view.point(at(2e5));
    view.point(at(3e5));
    expect(asked()).toEqual([1e5]);
    await answer(0);
    await tick();
    expect(view.readout).toBeNull(); // the first point's answer is not the latest point's
    expect(asked()).toEqual([1e5, 3e5]);
    await answer(1);
    await tick();
    expect(view.readout?.inputs).toEqual(at(3e5));
    view.point(["Hmass", -1e9, "P", 1e6]);
    await answer(2);
    await tick();
    expect([view.readout?.state, typeof view.readout?.refusal?.message]).toEqual([null, "string"]);
    // Leaving drops the flash in flight and the point that waited for it.
    view.point(at(4e5));
    view.point(at(5e5));
    view.leave();
    await answer(3);
    await tick();
    expect([view.readout, asked().length]).toEqual([null, 4]);
    view.point(at(6e5));
    expect(asked().at(-1)).toBe(6e5);
    await answer(4);
    await tick();
    expect(view.readout?.inputs).toEqual(at(6e5));
  });

  test("drops a late diagram, sampled or refused", async () => {
    const { engine, waiting, answer } = await held(["diagram"]);
    const route = $state({ params: new URLSearchParams("fluid=Water&d=pt") });
    const view = new DiagramView(engine);
    const stop = view.connect(route, () => {});
    flushSync();
    view.fluid = "Nitrogen";
    flushSync();
    await vi.waitFor(() => expect(waiting.length).toBe(2));
    await answer(1);
    await settle(view);
    const nitrogen = view.data;
    await answer(0);
    await vi.waitFor(() => expect(view.busy).toBe(false));
    await new Promise((r) => setTimeout(r, 500));
    expect(view.data).toBe(nitrogen);
    view.fluid = "Unobtainium";
    flushSync();
    view.fluid = "R134a";
    flushSync();
    await vi.waitFor(() => expect(waiting.length).toBe(4));
    await answer(3);
    await settle(view);
    await answer(2);
    await tick();
    expect([view.refusal, view.data?.lines.length ? "data" : "none"]).toEqual([null, "data"]);
    stop();
  });
});

describe("picking a state", { timeout: 30_000 }, () => {
  test("snapping and the grid's density are in the URL", async () => {
    const { view, writes, stop } = await setup("fluid=Water&d=ph&snap=1&grid=5");
    expect([view.snap, view.grid]).toEqual([true, 5]);
    view.toggleSnap();
    view.setGrid(2);
    flushSync();
    expect(last(writes)).toBe("fluid=Water&d=ph&grid=2");
    view.toggleSnap();
    view.setGrid(3);
    flushSync();
    expect(last(writes)).toBe("fluid=Water&d=ph&snap=1");
    for (const [grid, level] of [
      ["9", 5],
      ["0", 1],
      ["2.6", 3],
      ["x", 3],
      ["", 3],
      [" ", 3],
    ] as const) {
      view.load(new URLSearchParams(`grid=${grid}`));
      expect(view.grid, grid).toBe(level);
    }
    view.load(new URLSearchParams("snap=yes"));
    expect(view.snap).toBe(false);
    stop();
  });

  test("a view starts unsnapped, at the default density, before any route", async () => {
    const view = new DiagramView((await realEngine()).engine);
    expect([view.snap, view.grid, view.pinned]).toEqual([false, 3, false]);
    expect(view.params().toString()).toBe("fluid=Water&d=ph");
  });

  test("a pinned readout ignores the cursor, but not a step", async () => {
    const { view, stop } = await setup("fluid=Water&d=ph");
    const at = (p: number): [Key, number, Key, number] => ["Hmass", 400e3, "P", p];
    view.hover(at(1e6));
    await vi.waitFor(() => expect(view.readout?.inputs).toEqual(at(1e6)));
    view.togglePin();
    expect(view.pinned).toBe(true);
    view.hover(at(2e6));
    view.leave();
    await tick();
    expect(view.readout?.inputs).toEqual(at(1e6));
    view.point(at(3e6));
    await vi.waitFor(() => expect(view.readout?.inputs).toEqual(at(3e6)));
    view.togglePin();
    view.hover(at(4e6));
    await vi.waitFor(() => expect(view.readout?.inputs).toEqual(at(4e6)));
    view.togglePin();
    view.unpin();
    expect(view.pinned).toBe(false);
    view.leave();
    expect(view.readout).toBeNull();
    stop();
  });

  test("opens the readout's state in the calculator", async () => {
    const { view, stop } = await setup("fluid=Water&d=ph");
    expect(view.calculatorLink()).toBeNull();
    view.point(["Hmass", 400e3, "P", 1e6]);
    await vi.waitFor(() => expect(view.readout?.state).toBeTruthy());
    expect(view.calculatorLink()).toBe("#/calculator?fluid=Water&a=Hmass%3A400000&b=P%3A1000000");
    view.setSystem("engineering");
    expect(view.calculatorLink()).toBe("#/calculator?fluid=Water&a=Hmass%3A400000&b=P%3A1000000&units=engineering");
    stop();
  });
});

describe("the phase regions (PLAN.md M21.9)", { timeout: 30_000 }, () => {
  const PHASES = ["gas", "liquid", "supercritical", "supercritical_gas", "supercritical_liquid", "twophase"];
  const shown = (view: DiagramView) =>
    vi.waitFor(
      () => {
        flushSync();
        expect(view.phases).not.toBeNull();
      },
      { timeout: 20_000 },
    );

  test("are in the URL, sampled once per fluid, and let go when turned off", async () => {
    const { view, writes, calls, stop } = await setup("fluid=Water&d=ph&regions=1");
    expect(view.regions).toBe(true);
    await shown(view);
    const water = view.phases;
    expect(water?.regions.map((r) => r.phase).sort()).toEqual(PHASES);
    expect(calls.filter((c) => c === "regions").length).toBe(1);
    expect(calls.filter((c) => c === "state" || c === "batch")).toEqual([]);
    view.toggleRegions();
    flushSync();
    expect([view.regions, view.phases]).toEqual([false, null]);
    expect(last(writes)).toBe("fluid=Water&d=ph");
    const before = calls.length;
    view.toggleRegions();
    flushSync();
    expect(view.phases).toBe(water);
    expect(last(writes)).toBe("fluid=Water&d=ph&regions=1");
    view.choose("ts");
    await settle(view);
    expect(view.phases).toBe(water);
    expect(calls.slice(before).filter((c) => c === "regions")).toEqual([]);
    view.load(new URLSearchParams("regions=yes"));
    expect(view.regions).toBe(false);
    stop();
  });

  test("a new fluid's replace the old ones at once, and the old fluid's late answer is dropped", async () => {
    const { engine } = await realEngine();
    // Water's answers come late: each after a timer, so n-Propane's finish first.
    let [late, regions] = [0, 0];
    const slow: Engine = {
      call<B extends Body>(body: B): Promise<Values[B["op"]]> {
        const answer = engine.call(body);
        if (!("fluid" in body) || body.fluid !== "Water") return answer;
        late += 1;
        return new Promise((resolve, reject) =>
          setTimeout(() => {
            late -= 1;
            if (body.op === "regions") regions += 1;
            answer.then(resolve, reject);
          }, 5),
        );
      },
    };
    const route = $state({ params: new URLSearchParams("fluid=n-Propane&d=pt&regions=1") });
    const view = new DiagramView(slow);
    const stop = view.connect(route, () => undefined);
    await shown(view);
    const propane = view.phases;
    route.params = new URLSearchParams("fluid=Water&d=pt&regions=1");
    flushSync();
    expect(view.phases).toBeNull();
    route.params = new URLSearchParams("fluid=n-Propane&d=pt&regions=1");
    flushSync();
    expect(view.phases).toBe(propane);
    await vi.waitFor(() => expect([regions, late]).toEqual([1, 0]), { timeout: 20_000 });
    await new Promise((resolve) => setTimeout(resolve, 10));
    expect(view.phases).toBe(propane);
    // Water's were sampled, and kept: a view of Water shows them at once.
    route.params = new URLSearchParams("fluid=Water&d=pt&regions=1");
    flushSync();
    expect(Math.abs((view.phases?.critical?.T ?? 0) / 647.096 - 1)).toBeLessThan(1e-9);
    stop();
  });

  test("a fluid the package refuses has none", async () => {
    const { view, stop } = await setup("fluid=Nonsense&d=ph&regions=1");
    expect(view.refusal).not.toBeNull();
    await new Promise((resolve) => setTimeout(resolve, 50));
    expect(view.phases).toBeNull();
    stop();
  });
});

// A mixture's diagram (PLAN.md M21.5): R410A.mix's envelope on the real package, with R32's and R125's packs.
describe("a mixture's diagrams", { timeout: 30_000 }, () => {
  async function mixed(query: string) {
    const real = await realEngine();
    for (const file of ["r32.pack", "r125.pack"]) await real.engine.call({ op: "pack", bytes: packBytes(file) });
    const route = $state({ params: new URLSearchParams(query) });
    const writes: string[] = [];
    const view = new DiagramView(real.engine);
    const stop = view.connect(route, (p) => writes.push(p.toString()));
    await settle(view);
    return { view, route, writes, stop, ...real };
  }

  test("are its envelope on p–T and T–ρ, its components and the trace's end beside it", async () => {
    const { view, writes, calls, stop } = await mixed("fluid=R410A.mix&d=trho");
    expect([view.id, view.refusal, view.data?.lines]).toEqual(["trho", null, []]);
    expect(view.mixture).toEqual({
      components: ["R32", "R125"],
      feed: [expect.closeTo(0.6976, 4), expect.closeTo(0.3024, 4)],
      end: "closed",
      reason: "",
    });
    expect(view.data?.envelope).toBe(view.mixture);
    expect(view.data?.dome.liquid.length).toBeGreaterThan(10);
    // The model's critical point, from its constants, on T–ρ: (ρ_c, T_c).
    expect(view.data?.dome.critical?.inputs).toEqual([
      "Dmass",
      expect.closeTo(459.05, 2),
      "T",
      expect.closeTo(344.494, 3),
    ]);
    expect(last(writes)).toBe("fluid=R410A.mix&d=trho");
    view.choose("pt");
    await settle(view);
    expect([view.id, view.mixture?.end]).toEqual(["pt", "closed"]);
    expect(calls.filter((c) => c !== "pack")).toEqual(["diagram", "diagram"]);
    stop();
  });

  test("on a plane a mixture has not switch to p–T; a pure fluid's then has none", async () => {
    const { view, writes, calls, stop } = await mixed("fluid=R410A.mix&d=ph");
    // The package refused p–h, and only then was the engine asked whether R410A.mix is a mixture.
    expect(calls.filter((c) => c !== "pack")).toEqual(["diagram", "components", "diagram"]);
    expect([view.id, view.data?.envelope?.components]).toEqual(["pt", ["R32", "R125"]]);
    expect(last(writes)).toBe("fluid=R410A.mix&d=pt");
    view.choose("ph");
    await settle(view);
    expect(view.id).toBe("pt");
    // Its components were asked once, and kept.
    expect(calls.filter((c) => c === "components")).toEqual(["components"]);
    view.fluid = "Water";
    await settle(view);
    expect([view.mixture, view.data?.envelope]).toEqual([null, undefined]);
    view.fluid = "R410A.mix";
    await settle(view);
    expect(view.mixture?.components).toEqual(["R32", "R125"]);
    stop();
  });

  test("a name the engine refuses is no mixture, and the package's refusal is its diagram's", async () => {
    const { view, calls, stop } = await mixed("fluid=R410A.mix&d=pt");
    view.fluid = "R999.mix";
    await settle(view);
    expect([view.data, view.mixture]).toEqual([null, null]);
    expect(view.refusal).toContain("R999.mix");
    // On p–h too: a name the engine refuses is no mixture, so the view stays on p–h.
    view.choose("ph");
    await settle(view);
    expect([view.id, view.data]).toEqual(["ph", null]);
    expect(view.refusal).toContain("R999.mix");
    expect(calls.filter((c) => c === "components")).toEqual(["components"]);
    stop();
  });
});

describe("the curves and maps over a diagram (PLAN.md M26.6)", { timeout: 60_000 }, () => {
  test("reads and writes the curves and the map in the URL, and ignores an output the map does not offer", () => {
    const view = new DiagramView({ call: () => new Promise(() => {}) });
    view.load(new URLSearchParams("fluid=Water&d=pt&curves=1&map=Smass"));
    expect([view.curves, view.map]).toEqual([true, "Smass"]);
    expect(view.params().toString()).toBe("fluid=Water&d=pt&curves=1&map=Smass");
    view.load(new URLSearchParams("curves=yes&map=Phase"));
    expect([view.curves, view.map]).toEqual([false, null]);
    expect(view.params().toString()).toBe("fluid=Water&d=ph");
    view.setMap("Hmass");
    expect(view.map).toBe("Hmass");
    view.setMap("nothing");
    expect(view.map).toBeNull();
    view.toggleCurves();
    expect(view.curves).toBe(true);
  });

  test("asks for a fluid's curves once while they are shown, and none while they are not", async () => {
    const { view, calls, stop } = await setup("fluid=Nitrogen&d=pt&curves=1");
    await vi.waitFor(() => {
      flushSync();
      expect(view.curveData).not.toBeNull();
    });
    expect(view.curveData?.curves.map((c) => c.name)).toEqual([
      "Ideal",
      "Boyle",
      "JouleThomsonInversion",
      "JouleInversion",
    ]);
    view.toggleCurves();
    flushSync();
    expect(view.curveData).toBeNull();
    view.toggleCurves();
    flushSync();
    expect(view.curveData).not.toBeNull();
    expect(calls.filter((c) => c === "curves")).toHaveLength(1);
    stop();
  });

  test("asks for an output's map on the diagram's plane once, keeps it, and keeps a refusal in the package's words", async () => {
    const { view, calls, stop } = await setup("fluid=n-Propane&d=trho&map=P");
    await vi.waitFor(() => {
      flushSync();
      expect(view.mapData).not.toBeNull();
    });
    expect([view.mapData?.key, view.mapRefusal]).toEqual(["P", null]);
    expect(view.mapData?.levels.length).toBeGreaterThan(3);
    view.setMap(null);
    flushSync();
    expect(view.mapData).toBeNull();
    view.setMap("P");
    flushSync();
    expect(view.mapData?.key).toBe("P");
    expect(calls.filter((c) => c === "map")).toHaveLength(1);
    view.fluid = "Nothing";
    flushSync();
    await vi.waitFor(() => {
      flushSync();
      expect(view.mapRefusal).toMatch(/Nothing/);
    });
    expect(view.mapData).toBeNull();
    stop();
  });

  test("drops a map that arrives after another was chosen", async () => {
    const answers: ((v: unknown) => void)[] = [];
    const engine: Engine = {
      call<B extends Body>(body: B): Promise<Values[B["op"]]> {
        if (body.op !== "map") return new Promise(() => {});
        return new Promise((resolve) => answers.push(resolve as (v: unknown) => void));
      },
    };
    const route = $state({ params: new URLSearchParams("fluid=Water&d=ph&map=T") });
    const view = new DiagramView(engine);
    const stop = view.connect(route, () => {});
    flushSync();
    view.setMap("Smass");
    flushSync();
    const map = (key: string) => ({
      plane: "ph",
      key,
      values: new Float64Array([0, 1]),
      x: new Float64Array([0, 1]),
      y: new Float64Array([0, 1]),
      levels: new Float64Array(),
      contours: new Uint32Array([0]),
      lines: {
        starts: new Uint32Array([0]),
        vertices: { xy: new Float64Array(), inputs: new Float64Array(), pairs: new Uint8Array() },
      },
      low: new Float64Array([NaN]),
      high: new Float64Array([NaN]),
      bands: new Uint32Array([0, 0]),
      polygons: {
        starts: new Uint32Array([0]),
        vertices: { xy: new Float64Array(), inputs: new Float64Array(), pairs: new Uint8Array() },
      },
      pairKeys: [],
    });
    answers[1]?.(map("Smass"));
    answers[0]?.(map("T"));
    await vi.waitFor(() => {
      flushSync();
      expect(view.mapData?.key).toBe("Smass");
    });
    await new Promise((r) => setTimeout(r, 10));
    flushSync();
    expect(view.mapData?.key).toBe("Smass");
    stop();
  });
});

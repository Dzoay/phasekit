// The diagrams page's view (PLAN.md M21.2), with its wiring, under Node on the real package: the route loads the view,
// the view writes the URL, a fluid and diagram load their geometry once, and the cursor's readout is the package's
// flash at the pixel's inputs.
import { flushSync } from "svelte";
import { describe, expect, test, vi } from "vitest";
import { DiagramView, READOUT_KEYS } from "../../src/diagram/view.svelte";
import { createHandler } from "../../src/engine/handler";
import type { Body, Engine, Request, Values } from "../../src/engine/protocol";
import { loadPk } from "../pk";

async function realEngine(): Promise<{ engine: Engine; calls: string[] }> {
  const handle = createHandler(await loadPk("simd"), "simd128");
  const calls: string[] = [];
  let id = 0;
  const engine: Engine = {
    call<B extends Body>(body: B): Promise<Values[B["op"]]> {
      calls.push(body.op);
      const r = handle({ ...body, id: ++id } as Request);
      return r.ok ? Promise.resolve(r.value as Values[B["op"]]) : Promise.reject(new Error(r.error));
    },
  };
  return { engine, calls };
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

  test("refuses a fluid without a critical or triple point to draw from", async () => {
    const { engine: real } = await realEngine();
    const engine: Engine = {
      call: <B extends Body>(body: B) =>
        body.op === "constants"
          ? (Promise.resolve([1, 2, null, 4, 5, 6, 7, 8]) as Promise<Values[B["op"]]>)
          : real.call(body),
    };
    const route = $state({ params: new URLSearchParams("fluid=Water") });
    const view = new DiagramView(engine);
    const stop = view.connect(route, () => {});
    await settle(view);
    expect([view.data, view.refusal]).toEqual([null, "Water names no critical or triple point to draw a dome from"]);
    stop();
  });

  test("drops a late readout, and any from before the cursor left", async () => {
    const { engine, waiting, answer } = await held(["state"]);
    const view = new DiagramView(engine);
    view.point(["Hmass", 2.8e6, "P", 1e5]);
    view.point(["Hmass", 400e3, "P", 1e6]);
    await answer(1);
    await answer(0);
    await tick();
    expect(view.readout?.inputs).toEqual(["Hmass", 400e3, "P", 1e6]);
    view.point(["Hmass", -1e9, "P", 1e6]);
    view.point(["Hmass", 400e3, "P", 2e6]);
    await answer(3);
    await answer(2);
    await tick();
    expect([view.readout?.inputs, view.readout?.refusal]).toEqual([["Hmass", 400e3, "P", 2e6], null]);
    view.point(["Hmass", 400e3, "P", 3e6]);
    view.leave();
    view.point(["Hmass", -1e9, "P", 4e6]);
    await answer(4);
    await tick();
    expect(view.readout).toBeNull();
    expect(waiting.length).toBe(6);
  });

  test("drops a late diagram, sampled or refused", async () => {
    const { engine, answer } = await held(["constants"]);
    const route = $state({ params: new URLSearchParams("fluid=Water&d=pt") });
    const view = new DiagramView(engine);
    const stop = view.connect(route, () => {});
    flushSync();
    view.fluid = "Nitrogen";
    flushSync();
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
    await answer(3);
    await settle(view);
    await answer(2);
    await tick();
    expect([view.refusal, view.data?.lines.length ? "data" : "none"]).toEqual([null, "data"]);
    stop();
  });
});

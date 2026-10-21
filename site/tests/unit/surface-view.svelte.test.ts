// The surfaces page's view (PLAN.md M26.2): src/surface/view.svelte.ts, against the engine over the package, and
// against an engine whose answers the test releases.
import { flushSync } from "svelte";
import { describe, expect, test, vi } from "vitest";
import { createHandler } from "../../src/engine/handler";
import type { Body, Engine, Request, SurfaceValue, Values } from "../../src/engine/protocol";
import { SurfaceView } from "../../src/surface/view.svelte";
import { loadPk } from "../pk";

async function realEngine(): Promise<{ engine: Engine; calls: Body[] }> {
  const handle = createHandler(await loadPk("simd"), "simd128");
  const calls: Body[] = [];
  let id = 0;
  const engine: Engine = {
    call<B extends Body>(body: B): Promise<Values[B["op"]]> {
      calls.push(body);
      const r = handle({ ...body, id: ++id } as Request);
      return r.ok ? Promise.resolve(r.value as Values[B["op"]]) : Promise.reject(new Error(r.error));
    },
  };
  return { engine, calls };
}

const settle = (view: SurfaceView) =>
  vi.waitFor(
    () => {
      flushSync();
      expect(view.busy).toBe(false);
    },
    { timeout: 20_000 },
  );

async function setup(query: string) {
  const route = $state({ params: new URLSearchParams(query) });
  const writes: string[] = [];
  const { engine, calls } = await realEngine();
  const view = new SurfaceView(engine);
  const stop = view.connect(route, (p) => writes.push(p.toString()));
  await settle(view);
  return { view, route, writes, calls, stop };
}

describe("the surfaces view", { timeout: 30_000 }, () => {
  test("loads its view from the route and asks for the fluid's mesh once", async () => {
    const { view, writes, calls, stop } = await setup("fluid=Water&kind=usv");
    expect([view.fluid, view.kind, view.refusal]).toEqual(["Water", "usv", null]);
    expect(view.data?.T.length).toBeGreaterThan(1000);
    expect(writes.at(-1)).toBe("fluid=Water&kind=usv");
    view.choose("pvt");
    flushSync();
    expect(writes.at(-1)).toBe("fluid=Water");
    expect(calls).toEqual([{ op: "surface", fluid: "Water" }]);
    stop();
  });

  test("falls back to Water's p–v–T surface, and keeps a fluid's mesh when it comes back", async () => {
    const { view, route, calls, stop } = await setup("kind=nonsense");
    expect([view.fluid, view.kind]).toEqual(["Water", "pvt"]);
    const water = view.data;
    route.params = new URLSearchParams("fluid=CarbonDioxide");
    flushSync();
    expect(view.busy).toBe(true);
    await settle(view);
    expect(view.data).not.toBe(water);
    route.params = new URLSearchParams("fluid=Water");
    flushSync();
    expect([view.data, view.busy]).toEqual([water, false]);
    expect(calls.map((c) => (c.op === "surface" ? c.fluid : c.op))).toEqual(["Water", "CarbonDioxide"]);
    stop();
  });

  test("shows a refusal in the package's words, and no mesh", async () => {
    const { view, stop } = await setup("fluid=Nothing");
    expect(view.data).toBeNull();
    expect(view.refusal).toContain("Nothing");
    stop();
  });

  test("drops an answer for a fluid no longer chosen, a refusal too", async () => {
    const pending: { body: Body; resolve: (v: SurfaceValue) => void; reject: (e: unknown) => void }[] = [];
    const engine: Engine = {
      call<B extends Body>(body: B): Promise<Values[B["op"]]> {
        return new Promise((resolve, reject) => {
          pending.push({ body, resolve: resolve as (v: SurfaceValue) => void, reject });
        });
      },
    };
    const route = $state({ params: new URLSearchParams("fluid=A") });
    const view = new SurfaceView(engine);
    const stop = view.connect(route, () => {});
    flushSync();
    route.params = new URLSearchParams("fluid=B");
    flushSync();
    route.params = new URLSearchParams("fluid=C");
    flushSync();
    const mesh = { T: new Float64Array([1]) } as SurfaceValue;
    pending[0]?.resolve(mesh);
    pending[1]?.reject(new Error("B refused"));
    await vi.waitFor(() => expect([view.data, view.refusal, view.busy]).toEqual([null, null, true]));
    pending[2]?.reject("C refused");
    await vi.waitFor(() => expect([view.data, view.refusal, view.busy]).toEqual([null, "C refused", false]));
    // A's mesh was kept, though its answer came late.
    route.params = new URLSearchParams("fluid=A");
    flushSync();
    expect([view.data, view.refusal, view.busy]).toEqual([mesh, null, false]);
    expect(pending.length).toBe(3);
    stop();
  });
});

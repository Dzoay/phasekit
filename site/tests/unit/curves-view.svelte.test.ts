// The curves page's view (PLAN.md M26.6): src/curves/view.svelte.ts, against the engine over the package, and against
// an engine whose answers the test releases.
import { flushSync } from "svelte";
import { describe, expect, test, vi } from "vitest";
import { CurvesView } from "../../src/curves/view.svelte";
import { createHandler } from "../../src/engine/handler";
import type { Body, Engine, Request, Values } from "../../src/engine/protocol";
import { loadPk } from "../pk";
import { curvesOf, drain, emptyDiagram, heldEngine } from "./late";

async function realEngine(): Promise<{ engine: Engine; calls: Body[] }> {
  const handle = createHandler(await loadPk("simd"), "simd128");
  const calls: Body[] = [];
  let id = 0;
  const engine: Engine = {
    call<B extends Body>(body: B): Promise<Values[B["op"]]> {
      calls.push(body);
      const r = handle({ ...body, id: ++id } as Request);
      return r.ok
        ? Promise.resolve(r.value as Values[B["op"]])
        : Promise.reject(Object.assign(new Error(r.error.message), { refusal: r.error }));
    },
  };
  return { engine, calls };
}

const settle = (view: CurvesView) =>
  vi.waitFor(
    () => {
      flushSync();
      expect(view.busy).toBe(false);
    },
    { timeout: 20_000 },
  );

describe("the curves view", { timeout: 60_000 }, () => {
  test("is Nitrogen's, busy, before it connects", () => {
    const view = new CurvesView({ call: () => new Promise(() => {}) });
    expect([view.fluid, view.busy, view.data, view.refusal]).toEqual(["Nitrogen", true, null, null]);
    view.load(new URLSearchParams(""));
    expect(view.params().toString()).toBe("fluid=Nitrogen");
  });

  test("loads its fluid from the route, its curves and saturation line from two calls, kept for a return", async () => {
    const route = $state({ params: new URLSearchParams("fluid=CarbonDioxide") });
    const writes: string[] = [];
    const { engine, calls } = await realEngine();
    const view = new CurvesView(engine);
    const stop = view.connect(route, (p) => writes.push(p.toString()));
    await settle(view);
    expect([view.fluid, view.refusal, writes.at(-1)]).toEqual(["CarbonDioxide", null, "fluid=CarbonDioxide"]);
    expect(view.data?.curves.curves).toHaveLength(4);
    expect(view.data?.curves.chart.length).toBeGreaterThan(2);
    expect(view.data?.saturation.length).toBeGreaterThan(10);
    expect(view.data?.critical?.x).toBeCloseTo(304.13, 1);
    expect(calls.map((c) => c.op)).toEqual(["curves", "diagram"]);
    expect(calls[1]).toMatchObject({ plane: "pt" });
    const first = view.data;
    view.fluid = "Nitrogen";
    await settle(view);
    view.fluid = "CarbonDioxide";
    flushSync();
    expect([view.data, view.busy]).toEqual([first, false]);
    expect(calls).toHaveLength(4);
    stop();
  });

  test("shows a refusal in the package's words", async () => {
    const route = $state({ params: new URLSearchParams("fluid=Nothing") });
    const { engine } = await realEngine();
    const view = new CurvesView(engine);
    const stop = view.connect(route, () => {});
    await settle(view);
    expect([view.data, view.refusal?.message]).toEqual([null, expect.stringContaining("Nothing")]);
    stop();
  });

  test("drops an answer for a fluid no longer chosen", async () => {
    const pending: { body: Body; resolve: (v: unknown) => void; reject: (e: unknown) => void }[] = [];
    const engine: Engine = {
      call<B extends Body>(body: B): Promise<Values[B["op"]]> {
        return new Promise((resolve, reject) =>
          pending.push({ body, resolve: resolve as (v: unknown) => void, reject }),
        );
      },
    };
    const route = $state({ params: new URLSearchParams("fluid=Water") });
    const view = new CurvesView(engine);
    const stop = view.connect(route, () => {});
    flushSync();
    view.fluid = "Nitrogen";
    flushSync();
    for (const p of pending.slice(0, 2)) p.reject(new Error("late"));
    await new Promise((r) => setTimeout(r, 10));
    flushSync();
    expect([view.busy, view.refusal]).toEqual([true, null]);
    stop();
  });

  test("is busy again for a fluid it has not loaded, and drops a late answer for one no longer chosen", async () => {
    const { engine, pending } = heldEngine();
    const route = $state({ params: new URLSearchParams("fluid=Water") });
    const view = new CurvesView(engine);
    const stop = view.connect(route, () => {});
    flushSync();
    view.fluid = "Nitrogen";
    flushSync();
    // Nitrogen's answer first, then Water's, late.
    pending[2]?.resolve(curvesOf("Nitrogen"));
    pending[3]?.resolve(emptyDiagram());
    await drain();
    flushSync();
    expect([view.busy, view.data?.curves.curves[0]?.name]).toEqual([false, "Nitrogen"]);
    pending[0]?.resolve(curvesOf("Water"));
    pending[1]?.resolve(emptyDiagram());
    await drain();
    flushSync();
    expect(view.data?.curves.curves[0]?.name).toBe("Nitrogen");
    view.fluid = "Argon";
    flushSync();
    expect(view.busy).toBe(true);
    stop();
  });
});

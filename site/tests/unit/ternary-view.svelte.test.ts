// The ternary page's view (PLAN.md M26.6): src/ternary/view.svelte.ts, against the engine over the package with R32's
// and R125's packs, and against an engine whose answers the test releases.
import { flushSync } from "svelte";
import { describe, expect, test, vi } from "vitest";
import { createHandler } from "../../src/engine/handler";
import type { Body, Engine, Request, Values } from "../../src/engine/protocol";
import { TernaryView } from "../../src/ternary/view.svelte";
import { loadPk, packBytes } from "../pk";
import { drain, heldEngine, ternaryAt } from "./late";

async function realEngine(): Promise<{ engine: Engine; calls: Body[] }> {
  const handle = createHandler(await loadPk("simd"), "simd128");
  let id = 0;
  for (const file of ["r32.pack", "r125.pack"]) handle({ id: ++id, op: "pack", bytes: packBytes(file) });
  const calls: Body[] = [];
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

const settle = (view: TernaryView) =>
  vi.waitFor(
    () => {
      flushSync();
      expect(view.busy).toBe(false);
    },
    { timeout: 50_000 },
  );

describe("the ternary view", { timeout: 120_000 }, () => {
  test("is R407C's at 273.15 K and 5 bar, busy, before it connects; a malformed URL falls back to that", () => {
    const view = new TernaryView({ call: () => new Promise(() => {}) });
    expect([view.mixture, view.T, view.p, view.busy, view.data, view.choices]).toEqual([
      "R407C.mix",
      273.15,
      5e5,
      true,
      null,
      [],
    ]);
    view.load(new URLSearchParams("mixture=R404A.mix&T=250&p=3e5"));
    expect(view.params().toString()).toBe("mixture=R404A.mix&T=250&p=300000");
    for (const bad of ["T=-1&p=0", "T=x&p=", "T=Infinity&p=%20"]) {
      view.load(new URLSearchParams(bad));
      expect([view.T, view.p], bad).toEqual([273.15, 5e5]);
    }
    view.set("T", 260);
    view.set("p", 4e5);
    for (const bad of [0, -5, NaN, Infinity]) view.set("T", bad);
    expect([view.T, view.p]).toEqual([260, 4e5]);
  });

  test("offers the predefined mixtures of three components, and loads a diagram once for each mixture, T and p", async () => {
    const route = $state({ params: new URLSearchParams("T=273.15&p=400000") });
    const writes: string[] = [];
    const { engine, calls } = await realEngine();
    const view = new TernaryView(engine);
    const stop = view.connect(route, (p) => writes.push(p.toString()));
    await settle(view);
    expect(view.choices).toContain("R407C.mix");
    expect(view.choices).not.toContain("R410A.mix");
    expect([view.refusal, writes.at(-1)]).toEqual([null, "mixture=R407C.mix&T=273.15&p=400000"]);
    expect(view.data?.components).toEqual(["R32", "R125", "R134a"]);
    expect(view.data?.binodal.map((l) => l.bubble).sort()).toEqual([false, true]);
    expect(view.data?.region.length).toBeGreaterThan(10);
    const first = view.data;
    // Above the components' range every feed's equilibrium fails: a diagram with nothing in it, not a refusal.
    view.set("T", 1000);
    await settle(view);
    expect([view.refusal, view.data?.region, view.data?.binodal, view.data?.T]).toEqual([null, [], [], 1000]);
    view.set("T", 273.15);
    flushSync();
    expect([view.data, view.busy]).toEqual([first, false]);
    expect(calls.filter((c) => c.op === "ternary")).toHaveLength(2);
    stop();
  });

  test("shows a refusal in the package's words, and drops an answer for a mixture no longer chosen", async () => {
    const { engine } = await realEngine();
    const view = new TernaryView(engine);
    const route = $state({ params: new URLSearchParams("mixture=Water") });
    const stop = view.connect(route, () => {});
    await settle(view);
    expect([view.data, view.refusal?.message]).toEqual([null, expect.stringContaining("three-component")]);
    stop();

    const pending: ((e: unknown) => void)[] = [];
    const slow: Engine = {
      call<B extends Body>(body: B): Promise<Values[B["op"]]> {
        if (body.op === "mixtures") return Promise.reject(new Error("no list"));
        return new Promise((_, reject) => pending.push(reject));
      },
    };
    const late = new TernaryView(slow);
    const empty = $state({ params: new URLSearchParams("") });
    const stop2 = late.connect(empty, () => {});
    flushSync();
    late.set("p", 6e5);
    flushSync();
    pending[0]?.(new Error("late"));
    await new Promise((r) => setTimeout(r, 10));
    flushSync();
    expect([late.busy, late.refusal, late.choices]).toEqual([true, null, []]);
    stop2();
  });

  test("is busy again for a T it has not loaded, and drops a late answer for one no longer chosen", async () => {
    const { engine, pending } = heldEngine();
    const route = $state({ params: new URLSearchParams("T=250") });
    const view = new TernaryView(engine);
    const stop = view.connect(route, () => {});
    flushSync();
    view.set("T", 260);
    flushSync();
    pending[1]?.resolve(ternaryAt(260));
    await drain();
    flushSync();
    expect([view.busy, view.data?.T]).toEqual([false, 260]);
    pending[0]?.resolve(ternaryAt(250));
    await drain();
    flushSync();
    expect(view.data?.T).toBe(260);
    view.set("T", 270);
    flushSync();
    expect(view.busy).toBe(true);
    stop();
  });
});

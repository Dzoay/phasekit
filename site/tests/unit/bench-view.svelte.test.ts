// The benchmark page's view (PLAN.md M21.3) under Node, on engines that answer with set timings: its settings in the
// URL, a run on each build in turn with its states a second and cells by status, the builds' comparison, a refusal,
// and a run started meanwhile dropping the one before.
import { flushSync } from "svelte";
import { describe, expect, test, vi } from "vitest";
import { BENCH_OUTPUTS } from "../../src/bench/grids";
import { LIMIT_KEYS } from "../../src/sample/ranges";
import { BenchView, SIZES } from "../../src/bench/view.svelte";
import type { BatchValue, Body, Build, Engine, Values } from "../../src/engine/protocol";

// n-Propane's limits, in LIMIT_KEYS' order.
const PROPANE = [369.89, 4.2512e6, 220.4781, 85.525, 1.7203e-4, 85.525, 650, 1e9];

function fake(
  answer: (body: Extract<Body, { op: "bench" }>) => BatchValue,
  us = 2000,
  delay: number | ((body: Body) => number) = 0,
  constants: (number | null)[] = PROPANE,
) {
  const asked: Body[] = [];
  const engine: Engine = {
    call<B extends Body>(body: B): Promise<Values[B["op"]]> {
      asked.push(body);
      // The answer is made when it is due, so a refusal waits unmade rather than unhandled.
      const value = (): Promise<unknown> =>
        body.op === "constants"
          ? body.fluid === "Nonsense"
            ? Promise.reject(new Error("unknown fluid Nonsense"))
            : Promise.resolve(constants)
          : Promise.resolve({ timing: { us, runs: 3 }, batch: answer(body as Extract<Body, { op: "bench" }>) });
      const wait = typeof delay === "number" ? delay : delay(body);
      return (wait ? new Promise((r) => setTimeout(r, wait)).then(value) : value()) as Promise<Values[B["op"]]>;
    },
  };
  return { engine, asked };
}

const cells = (body: Extract<Body, { op: "bench" }>, status: number[] = []) => {
  const n = body.values1.length * body.outputs.length;
  return {
    values: Float64Array.from({ length: n }, (_, i) => i),
    status: Uint8Array.from({ length: n }, (_, i) => status[i] ?? 0),
    outputs: body.outputs.length,
  };
};

describe("the benchmark view", () => {
  test("keeps its settings in the URL, and falls back on anything else", () => {
    const view = new BenchView([]);
    const route = $state({ params: new URLSearchParams("fluid=Water&pair=QT&n=1000") });
    const writes: string[] = [];
    const stop = view.connect(route, (p) => writes.push(p.toString()));
    flushSync();
    expect([view.fluid, view.pair, view.n, view.key()]).toEqual(["Water", "QT", 1000, "Water/QT/1000"]);
    expect(writes.at(-1)).toBe("fluid=Water&pair=QT&n=1000");
    route.params = new URLSearchParams("pair=XY&n=12");
    flushSync();
    expect([view.fluid, view.pair, view.n]).toEqual(["n-Propane", "DT", 10_000]);
    expect(SIZES).toEqual([1000, 10_000, 100_000]);
    expect([new BenchView([]).running, new BenchView([]).run]).toEqual([false, null]);
    stop();
  });

  test("times each build in turn, counts its cells by status, and compares the two", async () => {
    const base = fake((b) => cells(b, [4, 0, 1, 4]), 2000);
    const simd = fake((b) => cells(b, [4, 0, 1, 4]), 500);
    const view = new BenchView([
      { build: "baseline", engine: base.engine },
      { build: "simd128", engine: simd.engine },
    ]);
    view.n = 1000;
    const running = view.start();
    expect(view.running).toBe(true);
    expect(view.run).toEqual({ key: "n-Propane/DT/1000", results: [], comparison: null, refusal: null });
    expect(base.asked[0]).toEqual({ op: "constants", fluid: "n-Propane", keys: [...LIMIT_KEYS] });
    await running;
    expect(view.running).toBe(false);
    const run = view.run;
    expect(run?.key).toBe("n-Propane/DT/1000");
    expect(run?.results.map((r) => [r.build, r.perSecond, r.timing.runs, r.counts])).toEqual([
      [
        "baseline",
        500_000,
        3,
        [
          [0, 3997],
          [1, 1],
          [4, 2],
        ],
      ],
      [
        "simd128",
        2_000_000,
        3,
        [
          [0, 3997],
          [1, 1],
          [4, 2],
        ],
      ],
    ]);
    expect(run?.comparison).toEqual({ cells: 4000, differing: 0, first: null });
    expect(run?.refusal).toBeNull();
    const bench = simd.asked.find((b) => b.op === "bench") as Extract<Body, { op: "bench" }>;
    expect([bench.fluid, bench.outputs, bench.name1, bench.values1.length]).toEqual([
      "n-Propane",
      [...BENCH_OUTPUTS],
      "T",
      1000,
    ]);
  });

  test("reports differing builds, and no comparison with one build", async () => {
    const shifted = fake((b) => ({ ...cells(b), values: cells(b).values.map((v) => (v === 7 ? 7.5 : v)) }));
    const plain = fake((b) => cells(b));
    const both = new BenchView([
      { build: "baseline", engine: plain.engine },
      { build: "simd128", engine: shifted.engine },
    ]);
    both.n = 1000;
    await both.start();
    expect(both.run?.comparison).toEqual({ cells: 4000, differing: 1, first: { cell: 7, a: 7, b: 7.5 } });
    const one = new BenchView([{ build: "baseline", engine: plain.engine }]);
    one.n = 1000;
    await one.start();
    expect([one.run?.results.length, one.run?.comparison, one.run?.refusal]).toEqual([1, null, null]);
    const blank = new BenchView([
      {
        build: "baseline",
        engine: fake(
          (b) => cells(b),
          1,
          0,
          PROPANE.map(() => null),
        ).engine,
      },
    ]);
    await blank.start();
    expect(blank.run?.refusal).toBe("n-Propane names no critical or triple point to place its states by");
  });

  test("shows a refusal, and drops a run's results when another starts", async () => {
    const view = new BenchView([{ build: "baseline", engine: fake((b) => cells(b)).engine }]);
    view.fluid = "Nonsense";
    await view.start();
    expect([view.run?.refusal, view.run?.results, view.running]).toEqual(["unknown fluid Nonsense", [], false]);
    const slow = fake((b) => cells(b), 1000, 30);
    const late = new BenchView([{ build: "simd128" as Build, engine: slow.engine }]);
    late.n = 1000;
    const first = late.start();
    late.pair = "PQ";
    const second = late.start();
    await Promise.all([first, second]);
    await vi.waitFor(() => expect(late.running).toBe(false));
    expect(late.run?.key).toBe("n-Propane/PQ/1000");
    expect(late.run?.results.length).toBe(1);
    // The first run's refusal after the second started is dropped too.
    const failing: Engine = {
      call<B extends Body>(body: B): Promise<Values[B["op"]]> {
        return (
          body.op === "constants"
            ? new Promise((_, reject) => setTimeout(() => reject(new Error("late")), 20))
            : Promise.resolve(PROPANE)
        ) as Promise<Values[B["op"]]>;
      },
    };
    const dropped = new BenchView([{ build: "baseline", engine: failing }]);
    const a = dropped.start();
    dropped.fluid = "Water";
    const b = dropped.start();
    await Promise.allSettled([a, b]);
    expect(dropped.run?.key).toBe("Water/DT/10000");
  });
});

describe("a run started while another is in flight", () => {
  // The first run's calls answer after 10 ms, the second's after 60: the first, set aside, finishes first.
  const timed = (fast: (b: Body) => boolean) => (b: Body) => (fast(b) ? 10 : 60);

  test("keeps the second's settings, results and running flag when the first finishes", async () => {
    const isDT = (b: Body) => b.op !== "bench" || b.name2 === "Dmass";
    const view = new BenchView([{ build: "baseline", engine: fake((b) => cells(b), 1000, timed(isDT)).engine }]);
    view.n = 1000;
    const first = view.start();
    view.pair = "PQ";
    const second = view.start();
    await first;
    expect([view.running, view.run?.key, view.run?.results]).toEqual([true, "n-Propane/PQ/1000", []]);
    await second;
    expect([view.running, view.run?.key, view.run?.results.length]).toEqual([false, "n-Propane/PQ/1000", 1]);
  });

  test("keeps the second's settings when the first is refused", async () => {
    const refused = (b: Body) => "fluid" in b && b.fluid === "Nonsense";
    const view = new BenchView([{ build: "baseline", engine: fake((b) => cells(b), 1000, timed(refused)).engine }]);
    view.n = 1000;
    view.fluid = "Nonsense";
    const first = view.start();
    view.fluid = "n-Propane";
    const second = view.start();
    await first;
    expect([view.running, view.run?.key, view.run?.refusal]).toEqual([true, "n-Propane/DT/1000", null]);
    await second;
  });
});

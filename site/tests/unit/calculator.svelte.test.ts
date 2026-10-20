// The calculator's view (PLAN.md M21.7), with its reactive wiring, under Node on the real package: the route loads the
// view, the view writes the URL, the inputs compute the state, and no edit is undone by the route. Each case of
// tests/e2e/interaction.spec.ts has its fast twin here, where StrykerJS can judge mutants by it.
import { flushSync } from "svelte";
import { describe, expect, test, vi } from "vitest";
import { createHandler } from "../../src/engine/handler";
import type { Body, Engine, Request, StateValue, Values } from "../../src/engine/protocol";
import { Calculator, KEYS, invalid } from "../../src/lib/calculator.svelte";
import { PRESETS } from "../../src/lib/fluids";
import { loadPk } from "../pk";

/** The engine as the page's worker runs it, called directly. */
async function realEngine(): Promise<Engine> {
  const handle = createHandler(await loadPk("simd"), "simd128");
  let id = 0;
  return {
    call<B extends Body>(body: B): Promise<Values[B["op"]]> {
      const response = handle({ ...body, id: ++id } as Request);
      return response.ok
        ? Promise.resolve(response.value as Values[B["op"]])
        : Promise.reject(new Error(response.error));
    },
  };
}

async function setup(query: string) {
  const route = $state({ params: new URLSearchParams(query) });
  const writes: string[] = [];
  const calc = new Calculator(await realEngine(), 0);
  const stop = calc.connect(route, (params) => writes.push(params.toString()));
  await settle(calc);
  return { calc, route, writes, stop };
}

/** Until the latest request has its answer. */
const settle = (calc: Calculator) =>
  vi.waitFor(() => {
    flushSync();
    expect(calc.busy).toBe(false);
  });

const value = (calc: Calculator, key: string) => calc.values.map.get(key) ?? Number.NaN;
const last = (writes: string[]) => writes.at(-1) ?? "";

describe("calculator", () => {
  test("loads its view from the route and computes it", async () => {
    const { calc, writes, stop } = await setup("fluid=Water&a=P:101325&b=Q:0");
    expect([calc.fluid, calc.aKey, calc.aValue, calc.bKey, calc.bValue, calc.system]).toEqual([
      "Water",
      "P",
      101325,
      "Q",
      0,
      "si",
    ]);
    expect([calc.aText, calc.bText]).toEqual(["101325", "0"]);
    expect(calc.result?.phase).toBe("twophase");
    expect(value(calc, "T")).toBeCloseTo(373.124, 3);
    expect(last(writes)).toBe("fluid=Water&a=P%3A101325&b=Q%3A0");
    stop();
  });

  test("a new first input variable keeps the state", async () => {
    const { calc, writes, stop } = await setup("fluid=Water&a=P:101325&b=Q:0");
    const [t, h] = [value(calc, "T"), value(calc, "Hmass")];
    calc.rekey("a", "Hmass");
    flushSync();
    expect([calc.aKey, calc.aValue]).toEqual(["Hmass", h]);
    expect(calc.aText).toMatch(/^419057\.7/);
    await settle(calc);
    expect(Math.abs(value(calc, "T") - t)).toBeLessThan(1e-6);
    expect(last(writes)).toContain(`a=Hmass%3A${h}`);
    stop();
  });

  test("a new second input variable keeps the state", async () => {
    const { calc, stop } = await setup("fluid=Water&a=T:300&b=P:101325");
    const rho = value(calc, "Dmass");
    calc.rekey("b", "Dmass");
    flushSync();
    expect([calc.bKey, calc.bValue]).toEqual(["Dmass", rho]);
    await settle(calc);
    expect(value(calc, "P") / 101325).toBeCloseTo(1, 6);
    stop();
  });

  test("a new input variable without a value keeps the old value", async () => {
    const { calc, stop } = await setup("fluid=Water&a=T:300&b=P:-5");
    expect(calc.result).toBeNull();
    calc.rekey("b", "Dmass");
    expect([calc.bKey, calc.bValue]).toEqual(["Dmass", -5]);
    stop();
  });

  test("typing recomputes, keeps the text, and ignores what is not a number", async () => {
    const { calc, writes, stop } = await setup("fluid=Water&a=T:300&b=P:101325");
    calc.typed("b", "2e5");
    flushSync();
    expect([calc.bText, calc.bValue]).toEqual(["2e5", 200000]);
    expect(last(writes)).toContain("b=P%3A200000");
    await settle(calc);
    expect(value(calc, "P") / 2e5).toBeCloseTo(1, 9);
    calc.typed("a", "-1.5e2");
    expect([calc.aText, calc.aValue]).toEqual(["-1.5e2", -150]);
    for (const text of ["abc", "", "  ", "1e", "Infinity"]) {
      calc.typed("a", text);
      expect([calc.aText, calc.aValue], text).toEqual([text, -150]);
      expect(invalid(text), text).toBe(true);
    }
    expect(invalid(" 42 ")).toBe(false);
    stop();
  });

  test("typing takes the shown unit", async () => {
    const { calc, stop } = await setup("fluid=Water&a=T:300&b=P:101325&units=engineering");
    calc.typed("a", "25");
    calc.typed("b", "2");
    expect([calc.aValue, calc.bValue]).toEqual([298.15, 2e5]);
    stop();
  });

  test("the units convert the input text, never the state", async () => {
    const { calc, writes, stop } = await setup("fluid=Water&a=P:101325&b=Q:0");
    const t = value(calc, "T");
    calc.setSystem("engineering");
    flushSync();
    expect([calc.system, calc.aText, calc.bText]).toEqual(["engineering", "1.01325", "0"]);
    expect([calc.unitOf("T").label, calc.unitOf("P").label, calc.unitOf("Hmass").label]).toEqual([
      "°C",
      "bar",
      "kJ/kg",
    ]);
    expect(last(writes)).toContain("units=engineering");
    await settle(calc);
    expect(value(calc, "T")).toBe(t);
    calc.setSystem("si");
    flushSync();
    expect([calc.aText, calc.unitOf("T").label]).toEqual(["101325", "K"]);
    expect(last(writes)).not.toContain("units");
    stop();
  });

  test("an unknown input key is labelled without a unit", async () => {
    const { calc, stop } = await setup("fluid=Water&a=P:101325&b=Q:0");
    expect(calc.unitOf("nonsense").label).toBe("");
    stop();
  });

  test("a preset loads its fluid and inputs", async () => {
    const { calc, writes, stop } = await setup("fluid=Water&a=P:101325&b=Q:0");
    const nitrogen = PRESETS.find((p) => p.fluid === "Nitrogen");
    if (!nitrogen) throw new Error("no nitrogen preset");
    calc.apply(nitrogen);
    flushSync();
    expect([calc.fluid, calc.aKey, calc.aValue, calc.bKey, calc.bValue]).toEqual(["Nitrogen", "T", 77, "P", 101325]);
    expect([calc.aText, calc.bText]).toEqual(["77", "101325"]);
    expect(last(writes)).toBe("fluid=Nitrogen&a=T%3A77&b=P%3A101325");
    await settle(calc);
    expect(calc.result?.phase).toBe("liquid");
    stop();
  });

  test("no edit is undone by the route, and a new route loads its view", async () => {
    const { calc, route, stop } = await setup("fluid=Water&a=T:300&b=P:100000");
    calc.fluid = "Nitrogen";
    calc.setSystem("engineering");
    calc.rekey("a", "Hmass");
    calc.typed("b", "3");
    flushSync();
    expect([calc.fluid, calc.system, calc.aKey, calc.bValue]).toEqual(["Nitrogen", "engineering", "Hmass", 3e5]);
    route.params = new URLSearchParams("fluid=R134a&a=T:300&b=Q:1");
    flushSync();
    expect([calc.fluid, calc.aKey, calc.aValue, calc.bKey, calc.bValue, calc.system]).toEqual([
      "R134a",
      "T",
      300,
      "Q",
      1,
      "si",
    ]);
    expect([calc.aText, calc.bText]).toEqual(["300", "1"]);
    stop();
  });

  test("a bad route falls back to the default view", async () => {
    const { calc, stop } = await setup("a=X:1&b=T:abc&units=furlongs");
    expect([calc.fluid, calc.aKey, calc.aValue, calc.bKey, calc.bValue, calc.system]).toEqual([
      "Water",
      "P",
      101325,
      "Q",
      0,
      "si",
    ]);
    calc.load(new URLSearchParams("a=T&b=T:"));
    expect([calc.aKey, calc.aValue, calc.bKey, calc.bValue]).toEqual(["P", 101325, "Q", 0]);
    stop();
  });

  test("params write the view and load reads it back", async () => {
    const { calc, stop } = await setup("fluid=CarbonDioxide&a=T:310&b=P:8000000&units=engineering");
    expect(calc.params().toString()).toBe("fluid=CarbonDioxide&a=T%3A310&b=P%3A8000000&units=engineering");
    const copy = new Calculator(await realEngine(), 0);
    copy.load(calc.params());
    expect([copy.fluid, copy.aKey, copy.aValue, copy.bKey, copy.bValue, copy.system]).toEqual([
      "CarbonDioxide",
      "T",
      310,
      "P",
      8e6,
      "engineering",
    ]);
    stop();
  });

  test("a refusal restates the inputs in SI and in the shown units", async () => {
    const { calc, stop } = await setup("fluid=Water&a=P:1002500000&b=T:0");
    expect([calc.result, calc.refusal]).toEqual([null, "invalid temperature: 0"]);
    expect(calc.received("T", 0)).toBe("T = 0 K");
    expect(calc.received("Q", 0.5)).toBe("q = 0.5 mol/mol");
    expect(calc.received("Delta", 2)).toBe("δ = 2");
    calc.setSystem("engineering");
    expect(calc.received("T", 0)).toBe("T = 0 K (−273.15 °C)");
    expect(calc.received("P", 1.0025e9)).toBe("p = 1.0025 × 10⁹ Pa (10025 bar)");
    expect(calc.received("nonsense", 1)).toBe("nonsense = 1");
    stop();
  });

  test("a refused state's timing is cleared, and a computed one is timed", async () => {
    const { calc, stop } = await setup("fluid=Water&a=P:101325&b=Q:0");
    await vi.waitFor(() => expect(calc.timing).not.toBeNull());
    expect(calc.timing?.flash.runs).toBeGreaterThan(1);
    calc.typed("b", "-1");
    await settle(calc);
    expect([calc.result, calc.timing]).toEqual([null, null]);
    stop();
  });

  test("the values split the answered outputs from the refused", async () => {
    const { calc, stop } = await setup("fluid=Water&a=T:300&b=P:101325");
    expect(calc.values.map.get("T")).toBe(300);
    expect(calc.values.map.has("fraction_min")).toBe(false);
    expect(calc.values.errors.get("fraction_min")).toMatch(/\S/);
    expect(calc.values.map.size + calc.values.errors.size).toBe(84);
    stop();
  });

  test("a late answer to an older request is dropped", async () => {
    const pending: { body: Body; resolve: (v: unknown) => void }[] = [];
    const engine: Engine = {
      call: <B extends Body>(body: B) =>
        new Promise<Values[B["op"]]>((resolve) => pending.push({ body, resolve: resolve as (v: unknown) => void })),
    };
    const route = $state({ params: new URLSearchParams("fluid=Water&a=T:300&b=P:1") });
    const calc = new Calculator(engine, 60_000);
    const stop = calc.connect(route, () => {});
    flushSync();
    calc.typed("b", "2");
    flushSync();
    const states = pending.filter((p) => p.body.op === "state");
    expect(states.length).toBe(2);
    const answer = (phase: string): StateValue => ({ phase, values: [], errors: [], mixture: null });
    states[1]?.resolve(answer("newer"));
    await vi.waitFor(() => expect(calc.result?.phase).toBe("newer"));
    states[0]?.resolve(answer("older"));
    await new Promise((r) => setTimeout(r, 0));
    expect(calc.result?.phase).toBe("newer");
    stop();
  });

  test("disconnecting stops the wiring", async () => {
    const { calc, route, writes, stop } = await setup("fluid=Water&a=P:101325&b=Q:0");
    stop();
    const count = writes.length;
    route.params = new URLSearchParams("fluid=R134a&a=T:300&b=Q:1");
    calc.typed("a", "200000");
    flushSync();
    expect([calc.fluid, writes.length]).toEqual(["Water", count]);
  });
});

/** An engine whose answers the test gives, in any order: each call waits until the test resolves or rejects it. */
function manualEngine() {
  const calls: { body: Body; resolve: (v: unknown) => void; reject: (e: unknown) => void }[] = [];
  const engine: Engine = {
    call: <B extends Body>(body: B) =>
      new Promise<Values[B["op"]]>((resolve, reject) =>
        calls.push({ body, resolve: resolve as (v: unknown) => void, reject }),
      ),
  };
  const of = (op: string) => calls.filter((c) => c.body.op === op);
  return { engine, of };
}

const answer = (phase: string, values: (number | null)[] = [], errors: (string | null)[] = []): StateValue => ({
  phase,
  values,
  errors,
  mixture: null,
});

describe("calculator, before and between answers", () => {
  test("a new calculator shows the default view and is busy until its first answer", async () => {
    const calc = new Calculator(await realEngine(), 0);
    expect([calc.fluid, calc.aKey, calc.aValue, calc.bKey, calc.bValue, calc.system]).toEqual([
      "Water",
      "P",
      101325,
      "Q",
      0,
      "si",
    ]);
    expect([calc.aText, calc.bText, calc.busy, calc.result, calc.refusal, calc.timing]).toEqual([
      "",
      "",
      true,
      null,
      null,
      null,
    ]);
  });

  test("an empty route is the default view", async () => {
    const { calc, stop } = await setup("");
    expect([calc.fluid, calc.aKey, calc.aValue, calc.bKey, calc.bValue]).toEqual(["Water", "P", 101325, "Q", 0]);
    stop();
  });

  test("is busy from a new request until its answer", async () => {
    const { engine, of } = manualEngine();
    const route = $state({ params: new URLSearchParams("fluid=Water&a=T:300&b=P:1") });
    const calc = new Calculator(engine, 0);
    const stop = calc.connect(route, () => {});
    flushSync();
    of("state")[0]?.resolve(answer("gas"));
    await vi.waitFor(() => expect(calc.busy).toBe(false));
    calc.typed("b", "2");
    flushSync();
    expect(calc.busy).toBe(true);
    stop();
  });

  test("an output with no value and no message is 'not defined at this state'", async () => {
    const { engine, of } = manualEngine();
    const route = $state({ params: new URLSearchParams("") });
    const calc = new Calculator(engine, 60_000);
    const stop = calc.connect(route, () => {});
    flushSync();
    const values = KEYS.map((k) => (k === "T" ? Number.NaN : 1));
    of("state")[0]?.resolve(
      answer(
        "gas",
        values,
        KEYS.map(() => null),
      ),
    );
    await vi.waitFor(() => expect(calc.result).not.toBeNull());
    expect(calc.values.errors.get("T")).toBe("not defined at this state");
    expect(calc.values.map.has("T")).toBe(false);
    stop();
  });

  test("a late refusal of an older request is dropped", async () => {
    const { engine, of } = manualEngine();
    const route = $state({ params: new URLSearchParams("fluid=Water&a=T:300&b=P:1") });
    const calc = new Calculator(engine, 60_000);
    const stop = calc.connect(route, () => {});
    flushSync();
    calc.typed("b", "2");
    flushSync();
    of("state")[1]?.resolve(answer("newer"));
    await vi.waitFor(() => expect(calc.result?.phase).toBe("newer"));
    of("state")[0]?.reject(new Error("older"));
    await new Promise((r) => setTimeout(r, 0));
    expect([calc.result?.phase, calc.refusal]).toEqual(["newer", null]);
    of("state")[1]?.reject("not an Error");
    stop();
  });

  test("a refusal that is not an Error is shown as text", async () => {
    const { engine, of } = manualEngine();
    const route = $state({ params: new URLSearchParams("") });
    const calc = new Calculator(engine, 0);
    const stop = calc.connect(route, () => {});
    flushSync();
    of("state")[0]?.reject("a bare string");
    await vi.waitFor(() => expect(calc.refusal).toBe("a bare string"));
    stop();
  });

  test("a settled state is timed once, a stale timing is dropped, and a failed one is ignored", async () => {
    vi.useFakeTimers();
    try {
      const { engine, of } = manualEngine();
      const route = $state({ params: new URLSearchParams("fluid=Water&a=T:300&b=P:1") });
      const calc = new Calculator(engine, 300);
      const stop = calc.connect(route, () => {});
      flushSync();
      of("state")[0]?.resolve(answer("first"));
      await vi.waitFor(() => expect(calc.result?.phase).toBe("first"));
      calc.typed("b", "2");
      flushSync();
      of("state")[1]?.resolve(answer("second"));
      await vi.waitFor(() => expect(calc.result?.phase).toBe("second"));
      await vi.advanceTimersByTimeAsync(300);
      expect(of("time").length).toBe(1); // the first state's timer was cleared when the second arrived
      expect(of("time")[0]?.body).toMatchObject({ op: "time", value2: 2 });
      calc.typed("b", "3");
      flushSync();
      of("time")[0]?.resolve({ flash: { us: 1, runs: 2 }, outputs: { us: 3, runs: 4 } });
      await vi.advanceTimersByTimeAsync(0);
      expect(calc.timing).toBeNull(); // its state is no longer the latest
      of("state")[2]?.resolve(answer("third"));
      await vi.waitFor(() => expect(calc.result?.phase).toBe("third"));
      await vi.advanceTimersByTimeAsync(300);
      of("time")[1]?.reject(new Error("timing failed"));
      await vi.advanceTimersByTimeAsync(0);
      expect([calc.timing, calc.refusal]).toEqual([null, null]);
      stop();
    } finally {
      vi.useRealTimers();
    }
  });

  test("an input that takes a value the package gave as infinite shows no text", async () => {
    const { engine, of } = manualEngine();
    const route = $state({ params: new URLSearchParams("") });
    const calc = new Calculator(engine, 60_000);
    const stop = calc.connect(route, () => {});
    flushSync();
    of("state")[0]?.resolve(
      answer(
        "gas",
        KEYS.map((k) => (k === "Hmass" ? Number.POSITIVE_INFINITY : 1)),
      ),
    );
    await vi.waitFor(() => expect(calc.result).not.toBeNull());
    calc.rekey("a", "Hmass");
    expect([calc.aValue, calc.aText]).toEqual([Number.POSITIVE_INFINITY, ""]);
    stop();
  });

  test("received names keys with subscripts plainly, and unknown keys bare", async () => {
    const calc = new Calculator(await realEngine(), 0);
    expect(calc.received("p_critical", 2.2e7)).toBe("pc = 2.2 × 10⁷ Pa");
    expect(calc.received("nonsense", 1.5)).toBe("nonsense = 1.5");
    expect(calc.unitOf("nonsense")).toEqual({ label: "", scale: 1, offset: 0 });
  });
});

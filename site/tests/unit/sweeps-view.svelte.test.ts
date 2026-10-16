// The sweeps page's view and chart (PLAN.md M21.3) under Node: the route loads the view and the view writes it, the
// held values are typed in the shown units, a sweep is sampled once per setting, and the chart's scales, lines, labels
// and nearest sample.
import { flushSync } from "svelte";
import { describe, expect, test, vi } from "vitest";
import { createHandler } from "../../src/engine/handler";
import type { Body, Engine, Request, Values } from "../../src/engine/protocol";
import { LATENT, type Sweep } from "../../src/sample/sweeps";
import { RAMP, buildChart, lineName, lineStyle, nearest } from "../../src/sweeps/chart";
import { SweepView, heldUnit, unitOfOutput } from "../../src/sweeps/view.svelte";
import { loadPk } from "../pk";

async function realEngine() {
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

const settle = (view: SweepView) =>
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
  const view = new SweepView(engine);
  const stop = view.connect(route, (p) => writes.push(p.toString()));
  await settle(view);
  return { view, route, writes, calls, stop };
}

const last = (writes: string[]) => writes.at(-1) ?? "";

describe("the sweeps view", { timeout: 30_000 }, () => {
  test("starts with n-Propane's c_p along isobars about p_c, in SI", async () => {
    const { view, writes, stop } = await setup("");
    expect([view.fluid, view.mode, view.y, view.system, view.at]).toEqual([
      "n-Propane",
      "isobars",
      "Cpmass",
      "si",
      null,
    ]);
    expect(view.data?.lines.map((l) => l.value)).toEqual([2.1e6, 4.7e6, 6.4e6, 1.3e7]);
    expect(view.heldText()).toBe("2100000, 4700000, 6400000, 13000000");
    expect(last(writes)).toBe("fluid=n-Propane&mode=isobars&y=Cpmass");
    stop();
  });

  test("starts busy, in SI, with nothing to show before any route", async () => {
    const view = new SweepView((await realEngine()).engine);
    expect([view.busy, view.system, view.data, view.heldText()]).toEqual([true, "si", null, ""]);
    expect(view.params().toString()).toBe("fluid=n-Propane&mode=isobars&y=Cpmass");
  });

  test("a bad route falls back, the latent heat only along saturation, and no held values there", async () => {
    const { view, stop } = await setup("mode=nonsense&y=nonsense&units=furlongs&at=x,-1,0");
    expect([view.mode, view.y, view.system, view.at]).toEqual(["isobars", "Cpmass", "si", null]);
    view.load(new URLSearchParams("mode=saturation&y=nonsense"));
    expect(view.y).toBe("Cpmass");
    view.load(new URLSearchParams(`mode=isobars&y=${LATENT}`));
    expect(view.y).toBe("Cpmass");
    view.load(new URLSearchParams(`mode=saturation&y=${LATENT}&at=5`));
    expect([view.mode, view.y, view.at]).toEqual(["saturation", LATENT, null]);
    view.load(new URLSearchParams("mode=isotherms&y=viscosity&at=300,400,0,500,600,700,800,900"));
    expect([view.mode, view.y, view.at]).toEqual(["isotherms", "viscosity", [300, 400, 500, 600, 700, 800]]);
    stop();
  });

  test("takes held values in the shown units, writes them in SI, and samples each setting once", async () => {
    const { view, writes, calls, stop } = await setup("fluid=n-Propane&mode=isobars&y=Cpmass&units=engineering");
    expect(view.heldText()).toBe("21, 47, 64, 130");
    view.setValues("20 47; 5");
    await settle(view);
    expect(view.at).toEqual([2e6, 4.7e6, 5e5]);
    expect(last(writes)).toBe("fluid=n-Propane&mode=isobars&y=Cpmass&units=engineering&at=2000000%2C4700000%2C500000");
    expect(view.data?.lines.map((l) => l.value)).toEqual([2e6, 4.7e6, 5e5]);
    expect(view.heldText()).toBe("20, 47, 5");
    const data = view.data;
    const batches = calls.filter((c) => c === "batch").length;
    view.setValues("x, -3");
    await settle(view);
    expect(view.at).toBeNull();
    view.setValues("20, 47, 5");
    await settle(view);
    expect(view.data).toBe(data);
    expect(calls.filter((c) => c === "batch").length).toBe(batches);
    view.setSystem("si");
    flushSync();
    expect(view.heldText()).toBe("2000000, 4700000, 500000");
    stop();
  });

  test("a new mode starts from its defaults, and leaves the latent heat behind", async () => {
    const { view, stop } = await setup(`fluid=Water&mode=saturation&y=${LATENT}`);
    expect(view.data?.lines.map((l) => l.held)).toEqual([null]);
    view.setMode("isotherms");
    await settle(view);
    expect([view.mode, view.y, view.at]).toEqual(["isotherms", "Cpmass", null]);
    expect(view.data?.lines.map((l) => l.value)).toEqual([580, 680, 780, 970]);
    view.setValues("300");
    view.setMode("isobars");
    expect(view.at).toBeNull();
    view.y = "viscosity";
    view.setMode("saturation");
    expect(view.y).toBe("viscosity");
    view.setMode("isotherms");
    expect(view.y).toBe("viscosity");
    view.y = LATENT;
    view.setMode("saturation");
    expect(view.y).toBe(LATENT);
    stop();
  });

  test("drops a late sweep, and a late refusal, that another setting has replaced", async () => {
    const { engine } = await realEngine();
    // Water's and Nonsense's answers come after 40 ms, the rest at once.
    const slow: Engine = {
      call<B extends Body>(body: B): Promise<Values[B["op"]]> {
        const late = "fluid" in body && (body.fluid === "Water" || body.fluid === "Nonsense");
        return late ? new Promise((r) => setTimeout(r, 40)).then(() => engine.call(body)) : engine.call(body);
      },
    };
    for (const first of ["Water", "Nonsense"]) {
      const route = $state({ params: new URLSearchParams(`fluid=${first}&mode=saturation&y=Hmass`) });
      const view = new SweepView(slow);
      const stop = view.connect(route, () => undefined);
      flushSync();
      route.params = new URLSearchParams("fluid=R134a&mode=saturation&y=Hmass");
      flushSync();
      await settle(view);
      const r134a = view.data;
      await new Promise((resolve) => setTimeout(resolve, 400));
      expect([view.data, view.refusal], first).toEqual([r134a, null]);
      stop();
    }
  });

  test("refuses a fluid with no critical or triple point to sweep about", async () => {
    const { engine } = await realEngine();
    const blank: Engine = {
      call<B extends Body>(body: B): Promise<Values[B["op"]]> {
        return body.op === "constants"
          ? (Promise.resolve(body.keys.map(() => null)) as Promise<Values[B["op"]]>)
          : engine.call(body);
      },
    };
    const route = $state({ params: new URLSearchParams("fluid=Water") });
    const view = new SweepView(blank);
    const stop = view.connect(route, () => undefined);
    await settle(view);
    expect(view.refusal).toBe("Water names no critical or triple point to sweep about");
    stop();
  });

  test("shows a fluid the package refuses in its words, and drops a late answer", async () => {
    const { view, route, stop } = await setup("fluid=Nonsense");
    expect([view.data, view.refusal?.includes("Nonsense")]).toEqual([null, true]);
    route.params = new URLSearchParams("fluid=Water&mode=saturation&y=Hmass");
    flushSync();
    route.params = new URLSearchParams("fluid=R134a&mode=saturation&y=Hmass");
    flushSync();
    await settle(view);
    await new Promise((resolve) => setTimeout(resolve, 200));
    expect(view.fluid).toBe("R134a");
    expect((view.data?.lines[0]?.points[0]?.x ?? 0) < 200).toBe(true);
    stop();
  });

  test("names each output's unit, and the held values' unit", () => {
    expect(unitOfOutput(LATENT, "engineering").label).toBe("kJ/kg");
    expect(unitOfOutput("viscosity", "si").label).toBe("Pa·s");
    expect([heldUnit("isobars", "engineering").label, heldUnit("isotherms", "engineering").label]).toEqual([
      "bar",
      "°C",
    ]);
    expect(heldUnit("saturation", "si").label).toBe("Pa");
  });
});

describe("the sweeps' chart", () => {
  const pt = (x: number, y: number) => ({ x, y, inputs: ["T", x, "P", 1] as const });
  const sweep: Sweep = {
    x: "T",
    lines: [
      { held: "P", value: 3e6, points: [pt(100, 10), pt(200, 20), null, pt(300, 30), pt(400, 40)] },
      { held: "P", value: 1e6, points: [pt(100, 5), pt(200, 6), null, null, null] },
    ],
  };
  // A line of three with a lone point after a gap, which draws nothing.
  const three: Sweep = {
    x: "T",
    lines: [{ held: "P", value: 1, points: [pt(128, 0), pt(192, 64), pt(256, 128), null, pt(320, 8)] }],
  };
  const options = { width: 400, height: 300, system: "si" as const, colour: (t: string) => `rgb(${t})` };

  test("draws each line in the ramp by its held value's rank, broken at failed cells", () => {
    const c = buildChart(sweep, "Hmass", options);
    expect(c.plot).toEqual({ left: 72, top: 16, right: 384, bottom: 252 });
    expect(c.lines.map((l) => [l.name, l.colour, l.segments.map((s) => s.length)])).toEqual([
      ["3000000 Pa", `rgb(${RAMP[1]})`, [2, 2]],
      ["1000000 Pa", `rgb(${RAMP[0]})`, [2]],
    ]);
    expect(c.titles).toEqual({ x: "T / K", y: "h / J/kg" });
    expect(c.lines.map((l) => l.dash)).toEqual([[], []]);
    expect(c.x.domain()).toEqual([88, 412]);
    expect(c.labels.map((l) => l.text)).toEqual(["3000000 Pa", "1000000 Pa"]);
    expect(c.yTicks.map((t) => t.text)).toEqual(["5", "10", "15", "20", "25", "30", "35", "40"]);
    expect(c.yTicks[0]?.at).toBeCloseTo(c.y(5), 12);
    const [first] = c.lines[1]?.segments[0] ?? [];
    expect(first?.[0]).toBeCloseTo(72 + ((100 - 88) / 324) * 312, 9);
    expect(first?.[1]).toBeCloseTo(c.y(5), 12);
  });

  test("labels a line at its last drawn point, and leaves out a lone point and a label with no room", () => {
    const c = buildChart(three, "Hmass", options);
    expect(c.lines[0]?.segments.map((s) => s.length)).toEqual([3]);
    const end = c.lines[0]?.segments[0]?.[2] as [number, number];
    // The last point is at the plot's top, so the label hangs below it.
    expect(c.labels).toEqual([{ x: end[0] + 4, y: end[1] + 14, text: "1 Pa", anchor: "start" }]);
    // Three lines on the same points: the third finds both its ends taken by the first two's labels.
    const line = three.lines[0] as Sweep["lines"][number];
    const thrice: Sweep = { x: "T", lines: [line, { ...line, value: 2 }, { ...line, value: 3 }] };
    expect(buildChart(thrice, "Hmass", options).labels.map((l) => l.text)).toEqual(["1 Pa", "2 Pa"]);
  });

  test("puts a log axis's ticks on its decades, and widens a flat line's domain", () => {
    const flat: Sweep = { x: "P", lines: [{ held: "T", value: 300, points: [pt(1e3, 2), pt(1e5, 2)] }] };
    const c = buildChart(flat, "Z", options);
    expect(c.xTicks.map((t) => t.text)).toEqual(["1000", "10⁴", "10⁵"]);
    // 2 ± 0.2, padded by 4 % of that span.
    expect(c.y.domain()[0]).toBeCloseTo(1.784, 12);
    expect(c.y.domain()[1]).toBeCloseTo(2.216, 12);
    const zero: Sweep = { x: "T", lines: [{ held: "P", value: 1, points: [pt(1, 0), pt(2, 0)] }] };
    expect(buildChart(zero, "Z", options).y.domain()).toEqual([-1.08, 1.08]);
    const none: Sweep = { x: "P", lines: [{ held: "T", value: 1, points: [null, null] }] };
    const empty = buildChart(none, "Z", options);
    expect([empty.x.domain(), empty.y.domain(), empty.lines[0]?.segments]).toEqual([[1, 10], [0, 1], []]);
  });

  test("names the saturated phases and the latent heat, in the isolines' first colours", () => {
    const saturation: Sweep = {
      x: "T",
      lines: [
        { held: "Q", value: 0, points: [] },
        { held: "Q", value: 1, points: [] },
      ],
    };
    expect([0, 1].map((i) => lineStyle(saturation, i))).toEqual([
      { colour: "--iso-1", dash: [] },
      { colour: "--iso-2", dash: [6, 4] },
    ]);
    expect(saturation.lines.map((l) => lineName(l, "si"))).toEqual(["liquid", "vapour"]);
    expect(lineName({ held: null, value: 0, points: [] }, "si")).toBe("h″ − h′");
    expect(lineStyle({ x: "T", lines: [{ held: null, value: 0, points: [] }] }, 0)).toEqual({
      colour: "--iso-1",
      dash: [],
    });
    expect(lineName({ held: "T", value: 300, points: [] }, "engineering")).toBe("26.85 °C");
    expect(buildChart(saturation, LATENT, options).titles.y).toBe("h″ − h′ / J/kg");
  });

  test("finds the sample nearest a value, in log space on a log axis, among those any line has", () => {
    expect(nearest(sweep, 240)).toBe(1);
    expect(nearest(sweep, 260)).toBe(3);
    // 247 is nearer 200 in value, 300 in log space; a tie goes to the first.
    expect(nearest(sweep, 247)).toBe(1);
    expect(nearest(sweep, 250)).toBe(1);
    expect(nearest(sweep, 1e9)).toBe(4);
    const logged: Sweep = { x: "P", lines: [{ held: "T", value: 1, points: [pt(1e3, 1), pt(1e5, 1)] }] };
    expect(nearest(logged, 2e4)).toBe(1);
    expect(nearest(logged, 5e3)).toBe(0);
    expect(nearest({ x: "T", lines: [] }, 1)).toBe(-1);
  });
});

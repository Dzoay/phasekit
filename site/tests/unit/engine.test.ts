// The engine the page's worker runs, under Node on the real package: it adds no numerics, so what it answers is the
// package's value, bit for bit (`Exact`, VERIFICATION.md §5).
import { describe, expect, test } from "vitest";
import { createHandler } from "../../src/engine/handler";
import { simdSupported } from "../../src/engine/simd";
import { loadPk, propsSI, sameBits } from "../pk";

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
      else expect([value, error], key).toEqual([null, want]);
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
    expect(!got.ok && got.error.length > 0 && got.error.includes("Unobtainium"), JSON.stringify(got)).toBe(true);
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
      expect(timing.runs).toBeGreaterThan(1);
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

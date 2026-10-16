// Sweeps (PLAN.md M21.3) under Node on the real package: every point is `PropsSI` at its inputs, bit for bit, and a
// failed cell is a gap where `PropsSI` refuses; along saturation the latent heat is the package's h″ less its h′.
import { describe, expect, test } from "vitest";
import { createHandler } from "../../src/engine/handler";
import type { Request } from "../../src/engine/protocol";
import type { Batch } from "../../src/sample/lines";
import { LIMIT_KEYS, limits, type Limits } from "../../src/sample/ranges";
import { LATENT, MAX_LINES, defaultValues, sampleSweep, type Sweep } from "../../src/sample/sweeps";
import { loadPk, propsSI, sameBits } from "../pk";

async function engine(fluid: string) {
  const handle = createHandler(await loadPk("simd"), "simd128");
  let id = 0;
  const batch: Batch = (outputs, name1, values1, name2, values2) => {
    const r = handle({ id: ++id, op: "batch", fluid, outputs, name1, values1, name2, values2 } as Request);
    return r.ok && r.op === "batch" ? Promise.resolve(r.value) : Promise.reject(new Error("refused"));
  };
  const c = handle({ id: ++id, op: "constants", fluid, keys: [...LIMIT_KEYS] });
  return { batch, l: limits(c.ok && c.op === "constants" ? c.value : []) as Limits };
}

/** Each point against `PropsSI` at its inputs; every `step`-th point and the line's ends. */
async function exact(sweep: Sweep, output: string, fluid: string, step = 17) {
  let checked = 0;
  for (const line of sweep.lines) {
    for (const [i, p] of line.points.entries()) {
      if (i % step !== 0 && i !== line.points.length - 1) continue;
      if (!p) continue;
      const want = await propsSI("simd", output, ...p.inputs, fluid);
      expect(sameBits(p.y, want), `${output} at ${p.inputs.join(" ")}`).toBe(true);
      checked += 1;
    }
  }
  return checked;
}

describe("a sweep", { timeout: 60_000 }, () => {
  test("along isobars is the package's output against T at each p, bit for bit", async () => {
    const { batch, l } = await engine("n-Propane");
    const at = defaultValues("isobars", l);
    expect(at).toEqual([2.1e6, 4.7e6, 6.4e6, 1.3e7]);
    const sweep = await sampleSweep("isobars", "Cpmass", at, l, batch, 120);
    expect(sweep.x).toBe("T");
    expect(sweep.lines.map((s) => [s.held, s.value, s.points.length])).toEqual(at.map((p) => ["P", p, 120]));
    for (const s of sweep.lines) {
      expect(
        s.points.every((p) => p === null || (p.inputs[0] === "T" && p.inputs[3] === s.value && p.x === p.inputs[1])),
      ).toBe(true);
    }
    expect(await exact(sweep, "Cpmass", "n-Propane")).toBeGreaterThan(20);
  });

  test("along isotherms is the output against p at each T, at most six lines", async () => {
    const { batch, l } = await engine("Water");
    const at = defaultValues("isotherms", l);
    expect(at).toEqual([580, 680, 780, 970]);
    const many = [...at, 1000, 1100, 1200];
    const sweep = await sampleSweep("isotherms", "viscosity", many, l, batch, 80);
    expect(sweep.x).toBe("P");
    expect(sweep.lines.map((s) => s.value)).toEqual(many.slice(0, MAX_LINES));
    expect(
      sweep.lines.every((s) => s.points.every((p) => p === null || (p.inputs[2] === "P" && p.x === p.inputs[3]))),
    ).toBe(true);
    expect(await exact(sweep, "viscosity", "Water")).toBeGreaterThan(20);
  });

  test("along saturation has a line for each phase, and the latent heat as h″ less h′", async () => {
    const { batch, l } = await engine("R134a");
    expect(defaultValues("saturation", l)).toEqual([]);
    const sweep = await sampleSweep("saturation", "speed_of_sound", [], l, batch, 60);
    expect(sweep.lines.map((s) => [s.held, s.value])).toEqual([
      ["Q", 0],
      ["Q", 1],
    ]);
    expect(await exact(sweep, "speed_of_sound", "R134a", 7)).toBeGreaterThan(10);
    const latent = await sampleSweep("saturation", LATENT, [], l, batch, 60);
    expect(latent.lines.map((s) => [s.held, s.points.length])).toEqual([[null, 60]]);
    for (const p of (latent.lines[0]?.points ?? []).filter((_, i) => i % 9 === 0)) {
      if (!p) throw new Error("no saturated state");
      const [, t] = p.inputs;
      expect(p.inputs).toEqual(["T", t, "Q", 1]);
      const [hv, hl] = [
        await propsSI("simd", "Hmass", "T", t, "Q", 1, "R134a"),
        await propsSI("simd", "Hmass", "T", t, "Q", 0, "R134a"),
      ];
      expect(sameBits(p.y, hv - hl)).toBe(true);
    }
  });

  test("leaves a gap where a cell failed", async () => {
    const { batch, l } = await engine("Water");
    const failing: Batch = async (outputs, name1, values1, name2, values2) => {
      const answer = await batch(outputs, name1, values1, name2, values2);
      const status = Uint8Array.from(answer.status);
      status[3] = 4;
      return { ...answer, status };
    };
    const sweep = await sampleSweep("isobars", "Hmass", [1e6], l, failing, 10);
    expect(sweep.lines[0]?.points.map((p) => p === null)).toEqual([
      false,
      false,
      false,
      true,
      false,
      false,
      false,
      false,
      false,
      false,
    ]);
    const latent = await sampleSweep("saturation", LATENT, [], l, failing, 10);
    expect(latent.lines[0]?.points[3]).toBeNull();
    expect(latent.lines[0]?.points[2]).not.toBeNull();
  });
});

// The calculator lists every output the package takes.
import { describe, expect, test } from "vitest";
import { INPUTS, OUTPUTS } from "../../src/lib/outputs";
import { loadPk } from "../pk";

describe("outputs", () => {
  test("are the package's 85 output keys, each once; Phase is the state's phase", async () => {
    const pk = await loadPk("simd");
    const keys = OUTPUTS.map((o) => o.key);
    expect(new Set(keys).size).toBe(keys.length);
    expect(new Set([...keys, "Phase"])).toEqual(new Set(pk.outputKeys()));
  });

  test("the input keys are outputs the calculator labels", () => {
    expect(INPUTS.length).toBeGreaterThan(0);
    for (const key of INPUTS)
      expect(
        OUTPUTS.some((o) => o.key === key),
        key,
      ).toBe(true);
  });
});

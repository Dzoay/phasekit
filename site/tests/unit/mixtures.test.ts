// What the page knows of mixing (PLAN.md M21.5): pairs by CAS number, the partners of chosen fluids, the predefined
// mixtures the package can build (on the real package and index: the page offers a mixture exactly where the package
// builds it), and mixture names read and written with each fraction as typed.
import { describe, expect, test } from "vitest";
import { buildable, liquidLiquid, nameOf, pairKey, pairs, partners, partsOf } from "../../src/lib/mixtures";
import { find, type PackEntry } from "../../src/lib/packs";
import { loadPk, packBytes, packIndex } from "../pk";

const entry = (name: string, cas: string | null): PackEntry => ({
  name,
  aliases: [name.toUpperCase()],
  cas,
  requires: [],
  pack: `${name.toLowerCase()}.pack`,
  bytes: 1,
});

const [a, b, c, d] = [entry("A", "75-10-5"), entry("B", "354-33-6"), entry("C", "811-97-2"), entry("D", null)];
// A pairs with B and C; B with C; nothing with D, which has no CAS number.
const LISTED = new Set([
  pairKey("75-10-5", "354-33-6"),
  pairKey("75-10-5", "811-97-2"),
  pairKey("354-33-6", "811-97-2"),
]);

describe("pairs", () => {
  test("a pair's key is its CAS numbers sorted as the package sorts them", () => {
    expect(pairKey("75-10-5", "354-33-6")).toBe("354-33-6&75-10-5");
    expect(pairKey("354-33-6", "75-10-5")).toBe("354-33-6&75-10-5");
  });

  test("two fluids pair where both have a CAS number and their pair is listed", () => {
    expect(pairs(a, b, LISTED)).toBe(true);
    expect(pairs(b, a, LISTED)).toBe(true);
    expect(pairs(a, d, LISTED)).toBe(false);
    expect(pairs(d, a, LISTED)).toBe(false);
    expect(pairs(a, b, new Set())).toBe(false);
  });

  // A key that names no CAS number would be `null&...`: listed here, it still pairs nothing.
  test("a fluid without a CAS number pairs with nothing, even where a listed key names null", () => {
    const nulls = new Set([...LISTED, "null&75-10-5", "75-10-5&null", "null&null"]);
    expect([pairs(a, d, nulls), pairs(d, a, nulls), pairs(d, d, nulls)]).toEqual([false, false, false]);
  });

  test("a fluid's partners mix with every fluid chosen, none of those among them", () => {
    expect(partners([a, b, c, d], [a], LISTED)).toEqual([b, c]);
    expect(partners([a, b, c, d], [a, b], LISTED)).toEqual([c]);
    expect(partners([a, b, c, d], [], LISTED)).toEqual([a, b, c, d]);
    expect(partners([a, b, c, d], [d], LISTED)).toEqual([]);
  });

  test("a predefined mixture is buildable where the index has every component and each two pair", () => {
    const predefined = [
      { name: "AB.mix", components: ["a", "B"] },
      { name: "ABC.mix", components: ["A", "B", "C"] },
      { name: "AD.mix", components: ["A", "D"] },
      { name: "AX.mix", components: ["A", "X"] },
    ];
    expect(buildable(predefined, [a, b, c, d], LISTED)).toEqual([
      { name: "AB.mix", components: [a, b] },
      { name: "ABC.mix", components: [a, b, c] },
    ]);
    const unpaired = new Set([pairKey("75-10-5", "354-33-6"), pairKey("75-10-5", "811-97-2")]);
    expect(buildable(predefined.slice(1, 2), [a, b, c], unpaired)).toEqual([]);
  });

  // Every predefined mixture's components' packs added to one registry: the page's buildable list is exactly the
  // mixtures the package builds, the rest refused for a missing pair (ROT-200).
  test("the page offers a predefined mixture exactly where the package builds it", { timeout: 120_000 }, async () => {
    const pk = await loadPk("simd");
    const fluids = packIndex().fluids;
    const predefined = pk.predefinedMixtures().map((name) => ({ name, components: pk.mixtureComponents(name) }));
    let registry = new pk.Registry();
    const served = new Set(registry.names());
    for (const name of new Set(predefined.flatMap((m) => m.components))) {
      const e = find(fluids, name);
      if (!e || served.has(e.name)) continue;
      served.add(e.name);
      const next = registry.withPack(packBytes(e.pack));
      registry.free();
      registry = next;
    }
    const offered = new Set(buildable(predefined, fluids, new Set(pk.binaryPairs())).map((m) => m.name));
    const built = predefined.filter(({ name }) => {
      try {
        registry.fluid(name).free();
        return true;
      } catch {
        return false;
      }
    });
    registry.free();
    expect([...offered].sort()).toEqual(built.map((m) => m.name).sort());
    expect(offered.has("R410A.mix")).toBe(true);
    expect(offered.size).toBeLessThan(predefined.length);
  });
});

describe("mixture names", () => {
  test("a mixture name's parts keep each fraction as written; an HEOS:: prefix is dropped", () => {
    const parts = [
      { name: "R32", fraction: "0.5" },
      { name: "R125", fraction: "5e-1" },
    ];
    expect(partsOf("R32[0.5]&R125[5e-1]")).toEqual(parts);
    expect(partsOf("HEOS::R32[0.5]&R125[5e-1]")).toEqual(parts);
    expect(partsOf("Water[1.0]")).toEqual([{ name: "Water", fraction: "1.0" }]);
    expect(partsOf("R32[]&R125[0.5]")).toEqual([
      { name: "R32", fraction: "" },
      { name: "R125", fraction: "0.5" },
    ]);
  });

  test("any other name has none", () => {
    for (const name of ["Water", "R410A.mix", "R32[0.5]&R125", "R32[0.5]x&R125[0.5]", "[0.5]&R125[0.5]", "R3[2[0.5]"])
      expect(partsOf(name), name).toBeNull();
  });

  test("parts write the name they were read from; one part alone is its fluid", () => {
    expect(nameOf(partsOf("R32[0.5]&R125[5e-1]") ?? [])).toBe("R32[0.5]&R125[5e-1]");
    expect(nameOf([{ name: "R32", fraction: "1" }])).toBe("R32");
  });
});

describe("a mixture's refusals", () => {
  // Methanol/benzene at x_MeOH = 0.54 splits into two liquids at 308.15 K and 1 atm (M13.8's corpus).
  test("of two liquids is the package's unsupported pair, and no other refusal is", async () => {
    const pk = await loadPk("simd");
    let registry = new pk.Registry();
    for (const file of ["methanol.pack", "benzene.pack"]) {
      const next = registry.withPack(packBytes(file));
      registry.free();
      registry = next;
    }
    let refusal = "";
    try {
      registry.PropsSI("D", "T", 308.15, "P", 101325, "Methanol[0.54]&Benzene[0.46]");
    } catch (e) {
      refusal = (e as Error).message;
    }
    registry.free();
    expect(refusal).toBe("input pair PT is not supported by this model");
    expect(liquidLiquid(refusal)).toBe(true);
    expect(liquidLiquid("input pair DmassT is not supported by this model")).toBe(true);
    for (const other of [
      "the vapour-liquid equilibrium solve (Vle) did not converge in 60 iterations",
      `${refusal}.`,
      `the ${refusal}`,
      "input pair  is not supported by this model",
    ])
      expect(liquidLiquid(other), other).toBe(false);
  });
});

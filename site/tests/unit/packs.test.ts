// The fluids beyond the five embedded (PLAN.md M21.4): a fluid string names an entry of packs/index.json as the
// registry resolves names; search finds entries by any name, letter case and punctuation aside; a fluid's packs come
// in order, each fluid it requires first, none the registry serves already.
import { describe, expect, test } from "vitest";
import { find, fold, plan, search, type PackEntry } from "../../src/lib/packs";
import { embedded, packIndex } from "../pk";

const entry = (name: string, more: Partial<PackEntry> = {}): PackEntry => ({
  name,
  aliases: [],
  cas: null,
  requires: [],
  pack: `${name.toLowerCase()}.pack`,
  bytes: 100,
  ...more,
});

describe("a fluid string", () => {
  const water = entry("Water", { aliases: ["H2O", "R718"], cas: "7732-18-5" });
  const r1234 = entry("R1234ze(E)");
  const fluids = [r1234, water];

  test("folds to its lower-case letters and digits", () => {
    expect([fold("R1234ze(E)"), fold("n-Propane"), fold("75-10-5"), fold("AIR.PPF")]).toEqual([
      "r1234zee",
      "npropane",
      "75105",
      "airppf",
    ]);
  });

  test("names an entry by its canonical name, CAS number or alias, in any letter case, and only so", () => {
    for (const name of ["Water", "WATER", "7732-18-5", "h2o", "R718"]) expect(find(fluids, name), name).toBe(water);
    expect(find(fluids, "r1234ze(e)")).toBe(r1234);
    for (const name of ["r1234zee", "Wat", "773218 5", "Unobtainium"]) expect(find(fluids, name), name).toBe(undefined);
  });
});

describe("search", () => {
  test("ranks a name that starts with the query, then an alias or CAS number that does, then names and aliases that contain it", () => {
    const fluids = [
      entry("Zz", { aliases: ["HeavyWater"] }),
      entry("Xwater"),
      entry("Yy", { aliases: ["WaterVapour"] }),
      entry("Water", { aliases: ["H2O"], cas: "7732-18-5" }),
    ];
    expect(search(fluids, "water").map((h) => [h.entry.name, h.via])).toEqual([
      ["Water", null],
      ["Yy", "WaterVapour"],
      ["Xwater", null],
      ["Zz", "HeavyWater"],
    ]);
  });

  test("keeps the index's order within a rank, and every entry for an empty query", () => {
    const fluids = [entry("Water-B"), entry("Water"), entry("Air")];
    expect(search(fluids, "wat").map((h) => h.entry.name)).toEqual(["Water-B", "Water"]);
    expect(search(fluids, " ").map((h) => [h.entry.name, h.via])).toEqual([
      ["Water-B", null],
      ["Water", null],
      ["Air", null],
    ]);
  });

  test("prefers the canonical name, then the CAS number, then the aliases in order; ignores letter case and punctuation", () => {
    const foo = entry("Foo", { aliases: ["Foobar"] });
    const q = entry("Q", { cas: "11-22-3", aliases: ["11-22-x", "1122y"] });
    expect(search([foo], "FOO")).toEqual([{ entry: foo, via: null }]);
    expect(search([q], "11 22")).toEqual([{ entry: q, via: "11-22-3" }]);
    expect(search([q], "1122y")).toEqual([{ entry: q, via: "1122y" }]);
    expect(search([q], "x")).toEqual([{ entry: q, via: "11-22-x" }]);
    expect(search([foo, q], "nothing")).toEqual([]);
    expect(search([foo], "e")).toEqual([]);
  });

  test("finds the index's fluids by name, alias and CAS number", () => {
    const fluids = packIndex().fluids;
    const first = (query: string) => {
      const hit = search(fluids, query)[0];
      return [hit?.entry.name, hit?.via];
    };
    expect(first("75-10-5")).toEqual(["R32", "75-10-5"]);
    expect(first("r744")).toEqual(["CarbonDioxide", "R744"]);
    expect(first("propane")).toEqual(["n-Propane", "Propane"]);
    expect(first("1234zee")).toEqual(["R1234ze(E)", null]);
    expect(search(fluids, "").length).toBe(136);
  });
});

describe("a fluid's packs", () => {
  const c = entry("C");
  const b = entry("B", { requires: ["C"] });
  const a = entry("A", { aliases: ["a-1"], requires: ["B"] });
  const d = entry("D", { requires: ["C", "C"] });
  const [e, f] = [entry("E", { requires: ["F"] }), entry("F", { requires: ["E"] })];
  const fluids = [a, b, c, d, e, f];
  const none = new Set<string>();

  test("come each fluid it requires first, with its own requirements before it, each once", () => {
    expect(plan(fluids, "A", none)).toEqual([c, b, a]);
    expect(plan(fluids, "a-1", none)).toEqual([c, b, a]);
    expect(plan(fluids, "D", none)).toEqual([c, d]);
    expect(plan(fluids, "E", none)).toEqual([f, e]);
  });

  test("leave out every fluid the registry serves, and a name the index lacks", () => {
    expect(plan(fluids, "A", new Set(["C"]))).toEqual([b, a]);
    expect(plan(fluids, "A", new Set(["B"]))).toEqual([a]);
    expect(plan(fluids, "A", new Set(["A"]))).toEqual([]);
    expect(plan(fluids, "Unobtainium", none)).toEqual([]);
  });

  test("of R32 are its own, its reference n-Propane embedded; with nothing embedded, n-Propane's first", async () => {
    const fluids = packIndex().fluids;
    expect(plan(fluids, "R32", new Set(await embedded())).map((p) => p.pack)).toEqual(["r32.pack"]);
    expect(plan(fluids, "R32", none).map((p) => p.pack)).toEqual(["n-propane.pack", "r32.pack"]);
  });
});

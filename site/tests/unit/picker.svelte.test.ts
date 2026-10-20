// The fluid picker's view (PLAN.md M21.4): it offers the registry's fluids until the index has come, then every fluid;
// closed, it shows the chosen fluid's canonical name; typing narrows the list; the arrows, Enter and Escape drive it.
import { describe, expect, test } from "vitest";
import { FluidData } from "../../src/lib/fluiddata.svelte";
import type { PackEntry } from "../../src/lib/packs";
import { Picker } from "../../src/lib/picker.svelte";

const entry = (name: string, more: Partial<PackEntry> = {}): PackEntry => ({
  name,
  aliases: [],
  cas: null,
  requires: [],
  pack: `${name.toLowerCase()}.pack`,
  bytes: 100,
  ...more,
});

const FLUIDS = [
  entry("CarbonDioxide", { aliases: ["R744", "co2"], cas: "124-38-9" }),
  entry("R32", { cas: "75-10-5", requires: ["n-Propane"] }),
  entry("R125"),
  entry("Water", { cas: "7732-18-5" }),
  entry("n-Propane", { aliases: ["Propane"] }),
];

function setup(value: string, fluids: readonly PackEntry[] = FLUIDS) {
  const data = new FluidData(
    { index: () => Promise.reject(new Error("unused")), pack: () => Promise.reject(new Error("unused")) },
    {
      add: () => Promise.reject(new Error("unused")),
      contents: () => Promise.reject(new Error("unused")),
      components: () => Promise.reject(new Error("unused")),
      mixtures: () => Promise.reject(new Error("unused")),
    },
  );
  data.names = ["Water", "n-Propane"];
  data.fluids = fluids;
  const box = $state({ value });
  const picker = new Picker(data, { get: () => box.value, set: (name) => (box.value = name) });
  return { picker, data, box };
}

const names = (picker: Picker) => picker.hits.map((h) => h.entry.name);

describe("the picker", () => {
  test("lists the predefined mixtures the package builds after the fluids, found by name or by a component", () => {
    const { picker, data } = setup("Water");
    expect(picker.entries).toBe(FLUIDS);
    data.mixable = {
      predefined: [
        { name: "R410A.mix", components: ["R32", "R125"] },
        { name: "R999.mix", components: ["R32", "Water"] },
      ],
      pairs: ["354-33-6&75-10-5"],
    };
    data.fluids = FLUIDS.map((f) => (f.name === "R125" ? { ...f, cas: "354-33-6" } : f));
    const r410a: PackEntry = {
      name: "R410A.mix",
      aliases: ["R32 + R125"],
      cas: null,
      requires: ["R32", "R125"],
      pack: "",
      bytes: 0,
      mixture: true,
    };
    expect(picker.entries).toEqual([...data.fluids, r410a]);
    picker.type("r410");
    expect(picker.hits).toEqual([{ entry: r410a, via: null }]);
    picker.type("r125");
    expect(names(picker)).toEqual(["R125", "R410A.mix"]);
    expect(picker.hits[1]?.via).toBe("R32 + R125");
    expect(data.cost(r410a)).toBe("mixture");
    picker.pick();
    expect(picker.text).toBe("R125");
    picker.type("410");
    picker.pick();
    expect(picker.text).toBe("R410A.mix");
  });

  test("offers the registry's fluids until the index has come, then the index's", () => {
    const { picker, data } = setup("Water", []);
    expect(picker.entries.map((e) => e.name)).toEqual(["Water", "n-Propane"]);
    expect(picker.entries[0]).toEqual(entry("Water", { pack: "", bytes: 0 }));
    data.fluids = FLUIDS;
    expect(picker.entries).toBe(FLUIDS);
  });

  test("closed, shows the chosen fluid's canonical name, or the text where no fluid answers to it", () => {
    const { picker, box } = setup("co2");
    expect([picker.query, names(picker)]).toEqual(["", FLUIDS.map((f) => f.name)]);
    expect([picker.open, picker.text]).toEqual([false, "CarbonDioxide"]);
    box.value = "Unobtainium";
    expect(picker.text).toBe("Unobtainium");
  });

  test("opens on every fluid, the chosen one highlighted, and shows what is typed", () => {
    const { picker } = setup("75-10-5");
    picker.show();
    expect([picker.open, picker.query, picker.text, picker.active]).toEqual([true, "", "", 1]);
    expect(names(picker)).toEqual(["CarbonDioxide", "R32", "R125", "Water", "n-Propane"]);
    const other = setup("Unobtainium").picker;
    other.active = 3;
    other.show();
    expect(other.active).toBe(0);
  });

  test("narrows to what is typed, its best hit highlighted, by name, alias or CAS number", () => {
    const { picker } = setup("Water");
    picker.show();
    picker.active = 2;
    picker.type("75-10");
    expect([picker.text, picker.active, names(picker)]).toEqual(["75-10", 0, ["R32"]]);
    expect(picker.hits[0]?.via).toBe("75-10-5");
    picker.type("R1");
    expect(names(picker)).toEqual(["R125"]);
    picker.type("propane");
    expect(picker.hits.map((h) => [h.entry.name, h.via])).toEqual([["n-Propane", "Propane"]]);
  });

  test("picks the highlighted fluid, or the one clicked, and closes", () => {
    const { picker, box } = setup("Water");
    picker.type("r");
    expect(names(picker)).toEqual(["R32", "R125", "CarbonDioxide", "Water", "n-Propane"]);
    picker.active = 1;
    picker.pick();
    expect([box.value, picker.open, picker.text]).toEqual(["R125", false, "R125"]);
    picker.show();
    picker.pick(FLUIDS[0]);
    expect([box.value, picker.open]).toEqual(["CarbonDioxide", false]);
    picker.type("zzz");
    picker.pick();
    expect([box.value, picker.open]).toEqual(["CarbonDioxide", false]);
  });

  test("closes without a change when dismissed", () => {
    const { picker, box } = setup("Water");
    picker.type("R32");
    picker.dismiss();
    expect([box.value, picker.open, picker.text]).toEqual(["Water", false, "Water"]);
  });

  test("takes the arrows, which open it and move the highlight within the list, Enter and Escape", () => {
    const { picker, box } = setup("R125");
    expect(picker.key("ArrowDown")).toBe(true);
    expect([picker.open, picker.active]).toEqual([true, 2]);
    picker.dismiss();
    expect(picker.key("ArrowUp")).toBe(true);
    expect([picker.open, picker.active]).toEqual([true, 2]);
    for (const [key, active] of [
      ["ArrowDown", 3],
      ["ArrowDown", 4],
      ["ArrowDown", 4],
      ["ArrowUp", 3],
    ] as const) {
      expect(picker.key(key)).toBe(true);
      expect(picker.active, key).toBe(active);
    }
    picker.active = 0;
    picker.key("ArrowUp");
    expect(picker.active).toBe(0);
    expect(picker.key("a")).toBe(false);
    expect(picker.key("Enter")).toBe(true);
    expect([box.value, picker.open]).toEqual(["CarbonDioxide", false]);
    expect([picker.key("Enter"), picker.key("Escape"), picker.key("a")]).toEqual([false, false, false]);
    picker.type("w");
    expect(picker.key("Escape")).toBe(true);
    expect([box.value, picker.open]).toEqual(["CarbonDioxide", false]);
  });

  test("keeps the highlight on an empty list at its start", () => {
    const { picker } = setup("Water");
    picker.type("zzz");
    picker.key("ArrowDown");
    expect(picker.active).toBe(0);
  });
});

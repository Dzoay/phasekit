// A refusal in the viewer's units (PLAN.md M28.3): the package's explanation in each unit system, or its message.
import { describe, expect, test } from "vitest";
import { refusalOf, remediesOf, remedyText, say, unitsOf } from "../../src/lib/refusal";
import { KINDS, SYSTEMS, displayUnit } from "../../src/lib/units";
import { loadPk } from "../pk";

/** A package refusal's handle, recording what it is asked and whether it is freed. */
function handle() {
  const asked: [string, string][] = [];
  let freed = 0;
  const refusal = {
    explain(units: Record<string, { label: string }>, style: string): string {
      asked.push([units.temperature?.label ?? "?", style]);
      return `${style} in ${units.temperature?.label ?? "?"}`;
    },
    free() {
      freed += 1;
    },
  };
  return { refusal, asked, freed: () => freed };
}

describe("refusals", () => {
  test("a system's units are each kind's shown unit", () => {
    for (const { id } of SYSTEMS) {
      const units = unitsOf(id);
      expect(Object.keys(units)).toEqual(KINDS.map((k) => k.id));
      for (const k of KINDS) expect(units[k.id]).toEqual(displayUnit(k.id, id));
    }
  });

  test("the package explains a refusal once per system and style, and its handle is freed", () => {
    const h = handle();
    const e = Object.assign(new Error("in SI"), { refusal: h.refusal });
    const r = refusalOf(e);
    expect(r).toEqual({
      message: "in SI",
      explained: {
        si: { long: "long in K", short: "short in K" },
        engineering: { long: "long in °C", short: "short in °C" },
      },
    });
    expect(h.asked).toEqual([
      ["K", "long"],
      ["K", "short"],
      ["°C", "long"],
      ["°C", "short"],
    ]);
    expect(h.freed()).toBe(1);
  });

  test("a refusal keeps the package's remedies and is explained with its subject (PLAN.md M29.7)", () => {
    const subjects: (string | undefined)[] = [];
    const refusal = {
      explain(_units: unknown, style: string, subject?: string): string {
        subjects.push(subject);
        return style;
      },
      free() {},
    };
    const e = Object.assign(new Error("m"), { refusal, remedies: ["extrapolate", 3, "other_pair"] });
    expect(refusalOf(e, "Water").remedies).toEqual(["extrapolate", "other_pair"]);
    expect(subjects).toEqual(["Water", "Water", "Water", "Water"]);
    expect(refusalOf(Object.assign(new Error("n"), { remedies: [] })).remedies).toBeUndefined();
    expect([remediesOf(null), remediesOf({ remedies: "x" }), remediesOf({})]).toEqual([[], [], []]);
  });

  test("a remedy in the site's terms, and none for one the site cannot follow", () => {
    for (const code of ["saturated_phases", "extrapolate", "other_pair", "declared_pair"]) {
      expect(remedyText(code)?.length, code).toBeGreaterThan(20);
    }
    expect(remedyText("other_pair")).toBe(remedyText("declared_pair"));
    expect(remedyText("saturated_phases")).toContain("quality to 0 or 1");
    for (const code of ["choose_root", "warm_start", "nearest_name", "nonsense"]) expect(remedyText(code)).toBeNull();
  });

  test("a handle that throws is still freed", () => {
    const h = handle();
    const broken = {
      ...h.refusal,
      explain: () => {
        throw new Error("no");
      },
    };
    expect(() => refusalOf(Object.assign(new Error("m"), { refusal: broken }))).toThrow("no");
    expect(h.freed()).toBe(1);
  });

  test("an engine's kept refusal comes back as it is; anything else is its message alone", () => {
    const kept = { message: "kept", explained: { si: { long: "l", short: "s" } } };
    expect(refusalOf(Object.assign(new Error("other"), { refusal: kept }))).toBe(kept);
    expect(refusalOf(new Error("plain"))).toEqual({ message: "plain", explained: {} });
    expect(refusalOf("a bare string")).toEqual({ message: "a bare string", explained: {} });
    expect(refusalOf(Object.assign(new Error("odd"), { refusal: 3 }))).toEqual({ message: "odd", explained: {} });
    expect(refusalOf(Object.assign(new Error("none"), { refusal: null }))).toEqual({ message: "none", explained: {} });
    expect(refusalOf(null)).toEqual({ message: "null", explained: {} });
  });

  test("a refusal is said in a system and a style, or as its message where it was not explained", () => {
    const r = { message: "m", explained: { engineering: { long: "words", short: "relation" } } };
    expect([say(r, "engineering", "long"), say(r, "engineering", "short"), say(r, "si", "long")]).toEqual([
      "words",
      "relation",
      "m",
    ]);
  });

  test("the package's own refusal, explained in the engineer's units", async () => {
    const pk = await loadPk("baseline");
    const registry = new pk.Registry();
    let thrown: unknown;
    try {
      registry.PropsSI("Hmass", "T", 0, "P", 1e5, "Water");
    } catch (e) {
      thrown = e;
    }
    registry.free();
    expect(refusalOf(thrown)).toEqual({
      message: "temperature 0 K is not above absolute zero",
      explained: {
        si: { long: "temperature 0 K is not above absolute zero", short: "T = 0 K ≤ 0 K" },
        engineering: {
          long: "temperature −273.15 °C is not above absolute zero",
          short: "T = −273.15 °C ≤ −273.15 °C",
        },
      },
    });
  });
});

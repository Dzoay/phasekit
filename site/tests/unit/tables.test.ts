// The page's tables: each output's quantity against CoolProp's units, and every name a viewer reads is written and
// tells its row apart. The presets are states the package computes, of a fluid of the index or a predefined mixture the
// package builds (PLAN.md M21.5).
import { describe, expect, test } from "vitest";
import { createHandler } from "../../src/engine/handler";
import { PRESETS } from "../../src/lib/fluids";
import { buildable } from "../../src/lib/mixtures";
import { GROUPS, HEADLINE, INPUTS, OUTPUTS, output } from "../../src/lib/outputs";
import { find } from "../../src/lib/packs";
import { ACCENTS, ACCENT_LABELS, MODES, MODE_LABELS } from "../../src/lib/theme";
import { KINDS, SYSTEMS, displayUnit, type KindId } from "../../src/lib/units";
import { embedded, loadPk, packBytes, packIndex } from "../pk";
import { COOLPROP_UNITS } from "./coolprop-units";

/** A unit label in CoolProp's notation: `J/(mol·K)` is `J/mol/K`, `kg/m³` is `kg/m^3`, `Pa·s` is `Pa-s`. */
const coolprop = (label: string): string =>
  label === ""
    ? "-"
    : label
        .replace(/\((\w+)·(\w+)\)/, "$1/$2")
        .replace("·", "-")
        .replace("³", "^3");

// The virial coefficients, which CoolProp leaves without a unit.
const DIMENSIONED: Readonly<Record<string, string>> = {
  Bvirial: "m³/mol",
  Cvirial: "m⁶/mol²",
  dBvirial_dT: "m³/(mol·K)",
  dCvirial_dT: "m⁶/(mol²·K)",
};

const distinct = (xs: readonly string[]): boolean => new Set(xs).size === xs.length;
const written = (xs: readonly string[]): boolean => xs.every((x) => x.trim() !== "");

describe("tables", () => {
  test("each output's SI unit is CoolProp's, the virial coefficients' dimensions aside", async () => {
    const pk = await loadPk("simd");
    expect(Object.keys(COOLPROP_UNITS).sort()).toEqual([...pk.outputKeys()].sort());
    for (const o of OUTPUTS) {
      const si = displayUnit(o.kind, "si").label;
      if (o.key in DIMENSIONED) expect(si, o.key).toBe(DIMENSIONED[o.key]);
      else expect(coolprop(si), o.key).toBe(COOLPROP_UNITS[o.key]);
    }
  });

  test("the outputs' symbols are distinct and every name is written", () => {
    expect(distinct(OUTPUTS.map((o) => o.symbol))).toBe(true);
    expect(written(OUTPUTS.flatMap((o) => [o.key, o.symbol, o.label]))).toBe(true);
    expect(distinct(GROUPS.map((g) => g.title)) && written(GROUPS.map((g) => g.title))).toBe(true);
    expect(GROUPS.every((g) => g.outputs.length > 0)).toBe(true);
    expect(output("nonsense")).toBeUndefined();
    expect(output("T")?.label).toBe("Temperature");
  });

  test("the headline and the inputs are distinct outputs", () => {
    for (const keys of [HEADLINE, INPUTS]) {
      expect(distinct(keys)).toBe(true);
      expect(keys.every((k) => output(k) !== undefined)).toBe(true);
    }
    expect(HEADLINE).toEqual(["T", "P", "Dmass", "Hmass", "Smass", "Q"]);
  });

  test("the presets' names are written and distinct; presets name fluids of the index or mixtures and input keys", async () => {
    expect(distinct(PRESETS.map((p) => p.label)) && written(PRESETS.map((p) => p.label))).toBe(true);
    const fluids = packIndex().fluids;
    const pk = await loadPk("simd");
    const predefined = pk.predefinedMixtures().map((name) => ({ name, components: pk.mixtureComponents(name) }));
    const mixtures = buildable(predefined, fluids, new Set(pk.binaryPairs())).map((m) => m.name);
    for (const p of PRESETS) {
      expect(fluids.some((f) => f.name === p.fluid) || mixtures.includes(p.fluid), p.label).toBe(true);
      expect(INPUTS.includes(p.a[0]) && INPUTS.includes(p.b[0]) && p.a[0] !== p.b[0], p.label).toBe(true);
    }
  });

  test("each preset is a state the package computes, one from its pack and two mixtures from their components'", async () => {
    const pk = await loadPk("simd");
    const handle = createHandler(pk, "simd128");
    const named = await embedded();
    const packed = PRESETS.filter((p) => !named.includes(p.fluid));
    expect(packed.map((p) => p.fluid)).toEqual(["R32", "R410A.mix", "Air.mix"]);
    const needed = new Set(
      packed.flatMap((p) => (p.fluid.endsWith(".mix") ? pk.mixtureComponents(p.fluid) : [p.fluid])),
    );
    const fluids = packIndex().fluids;
    for (const name of new Set([...needed].map((n) => find(fluids, n)?.name ?? n))) {
      if (named.includes(name)) continue;
      const entry = find(fluids, name);
      expect(handle({ id: 0, op: "pack", bytes: packBytes(entry?.pack ?? "") }).ok, name).toBe(true);
    }
    for (const [i, p] of PRESETS.entries()) {
      const [[name1, value1], [name2, value2]] = [p.a, p.b];
      const got = handle({ id: i, op: "state", fluid: p.fluid, name1, value1, name2, value2, outputs: ["T"] });
      expect(got.ok, `${p.label}: ${JSON.stringify(got)}`).toBe(true);
    }
  });

  test("the engineer's units are °C, bar, kJ and their kin; SI is each kind's first", () => {
    const engineering: Partial<Record<KindId, string>> = {
      temperature: "°C",
      pressure: "bar",
      molarDensity: "mol/dm³",
      specificEnergy: "kJ/kg",
      molarEnergy: "kJ/mol",
      specificEntropy: "kJ/(kg·K)",
      molarMass: "g/mol",
      viscosity: "µPa·s",
      conductivity: "mW/(m·K)",
      surfaceTension: "mN/m",
      inversePressure: "1/MPa",
    };
    for (const kind of KINDS) {
      const si = kind.units[0]?.label;
      expect(displayUnit(kind.id, "si").label, kind.id).toBe(si);
      expect(displayUnit(kind.id, "engineering").label, kind.id).toBe(engineering[kind.id] ?? si);
    }
    expect(SYSTEMS.map((s) => s.id)).toEqual(["si", "engineering"]);
    expect(written(SYSTEMS.map((s) => s.label)) && distinct(SYSTEMS.map((s) => s.label))).toBe(true);
  });

  test("a kind's units are distinct, and only the dimensionless one has no label", () => {
    expect(distinct(KINDS.map((k) => k.id))).toBe(true);
    for (const kind of KINDS) {
      const labels = kind.units.map((u) => u.label);
      expect(distinct(labels), kind.id).toBe(true);
      expect(written(labels), kind.id).toBe(kind.id !== "dimensionless");
    }
  });

  test("the theme controls' names are written and distinct", () => {
    for (const labels of [MODES.map((m) => MODE_LABELS[m]), ACCENTS.map((a) => ACCENT_LABELS[a])]) {
      expect(written(labels) && distinct(labels)).toBe(true);
    }
  });
});

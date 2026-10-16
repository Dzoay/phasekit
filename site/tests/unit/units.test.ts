// The calculator's units against the SI brochure (BIPM, 9th edition, 2019): 0 °C is 273.15 K exactly (§2.3.1), 1 bar is
// 10⁵ Pa (table 8), and the prefixes m, k and M are 10⁻³, 10³ and 10⁶ (table 7).
import { describe, expect, test } from "vitest";
import { KINDS, fromSI, toSI, unit } from "../../src/lib/units";

describe("units", () => {
  test("match the SI definitions", () => {
    expect(toSI(0, unit("temperature", "°C"))).toBe(273.15);
    expect(fromSI(273.15, unit("temperature", "°C"))).toBe(0);
    expect(toSI(1, unit("pressure", "bar"))).toBe(1e5);
    expect(toSI(1, unit("pressure", "kPa"))).toBe(1e3);
    expect(toSI(1, unit("pressure", "MPa"))).toBe(1e6);
    expect(toSI(1, unit("specificEnergy", "kJ/kg"))).toBe(1e3);
    expect(toSI(1, unit("specificEntropy", "kJ/(kg·K)"))).toBe(1e3);
    expect(toSI(1, unit("molarDensity", "mol/dm³"))).toBe(1e3);
    expect(toSI(1, unit("viscosity", "mPa·s"))).toBe(1e-3);
    expect(toSI(1, unit("molarMass", "g/mol"))).toBe(1e-3);
  });

  test("every kind has its SI unit first, at scale 1 and offset 0", () => {
    for (const kind of KINDS) {
      const first = kind.units[0];
      expect(first, kind.id).toBeDefined();
      expect([first?.scale, first?.offset], kind.id).toEqual([1, 0]);
    }
  });

  test("round-trip every unit to within 4 ulp of the larger magnitude", () => {
    for (const kind of KINDS) {
      for (const u of kind.units) {
        for (const x of [-40, 0, 1e-3, 1, 300, 101325, 6.02214076e23]) {
          const si = toSI(x, u);
          const back = fromSI(si, u);
          const tolerance = 4 * Number.EPSILON * Math.max(Math.abs(x), Math.abs(si));
          expect(Math.abs(back - x), `${kind.id} ${u.label} ${x}`).toBeLessThanOrEqual(tolerance);
        }
      }
    }
  });
});

describe("unit lookup", () => {
  test("names what is missing", () => {
    expect(() => unit("temperature", "°F")).toThrow("no unit °F of temperature");
  });
});

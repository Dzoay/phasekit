// How the page writes numbers and durations.
import { describe, expect, test } from "vitest";
import { formatNumber, formatTime } from "../../src/lib/format";
import { parseRoute } from "../../src/lib/routes";

describe("format", () => {
  test("numbers: seven significant digits, scientific outside [10⁻³, 10⁷), a true minus sign", () => {
    const cases: [number, string][] = [
      [Number.NaN, "—"],
      [Number.POSITIVE_INFINITY, "∞"],
      [Number.NEGATIVE_INFINITY, "−∞"],
      [0, "0"],
      [373.12429584766653, "373.1243"],
      [-40, "−40"],
      [1e-3, "0.001"],
      [9999999, "9999999"],
      [9.99e-4, "9.99 × 10⁻⁴"],
      [1e7, "1 × 10⁷"],
      [1.0025e9, "1.0025 × 10⁹"],
      [-1.2345678e-5, "−1.234568 × 10⁻⁵"],
      [2.5e10, "2.5 × 10¹⁰"],
      [6.02214076e23, "6.022141 × 10²³"],
      [1e-300, "1 × 10⁻³⁰⁰"],
      [1e-6, "1 × 10⁻⁶"],
      [3e8, "3 × 10⁸"],
      [-0.5, "−0.5"],
      [123456.789, "123456.8"],
    ];
    for (const [x, text] of cases) expect(formatNumber(x), String(x)).toBe(text);
  });

  test("durations: three significant digits, in ms once they round to a millisecond", () => {
    const cases: [number, string][] = [
      [0.5, "0.500 µs"],
      [5, "5.00 µs"],
      [70.6, "70.6 µs"],
      [999.4, "999 µs"],
      [999.6, "1.00 ms"],
      [1000, "1.00 ms"],
      [4880, "4.88 ms"],
      [123456, "123 ms"],
    ];
    for (const [us, text] of cases) expect(formatTime(us), String(us)).toBe(text);
  });
});

describe("routes", () => {
  test("an empty hash is the calculator's", () => {
    for (const hash of ["", "#", "#/", "#/?"]) expect(parseRoute(hash).page, hash).toBe("calculator");
  });

  test("a hash names its page and params", () => {
    const route = parseRoute("#/calculator?fluid=Water&a=P%3A101325");
    expect([route.page, route.params.get("fluid"), route.params.get("a")]).toEqual(["calculator", "Water", "P:101325"]);
    expect(parseRoute("#/licences").page).toBe("licences");
    expect(parseRoute("#licences").page).toBe("licences");
    expect([...parseRoute("#/licences").params]).toEqual([]);
    expect(parseRoute("#//x").page).toBe("/x");
    expect(parseRoute("#/p?x=1?y=2").params.get("x")).toBe("1?y=2");
    expect(parseRoute("a#/b").page).toBe("a#/b");
    expect(parseRoute("#/?fluid=Water").params.get("fluid")).toBe("Water");
  });
});

// The themes against WCAG 2.2: text at least 4.5:1 on its background (success criterion 1.4.3, AA) and the borders of
// controls at least 3:1 (1.4.11), with the standard's contrast ratio and relative luminance.
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, test } from "vitest";
import { contrast, luminance } from "../../src/lib/contrast";
import { themes } from "./tokens";

// From the site directory: see tests/pk.ts.
const css = readFileSync(resolve("src/app.css"), "utf8");

// [foreground, background, minimum ratio]
const PAIRS: [string, string, number][] = [
  ["--fg", "--bg", 4.5],
  ["--fg", "--surface", 4.5],
  ["--fg", "--surface-2", 4.5],
  ["--fg-muted", "--bg", 4.5],
  ["--fg-muted", "--surface", 4.5],
  ["--fg-muted", "--surface-2", 4.5],
  ["--accent", "--bg", 4.5],
  ["--accent", "--surface", 4.5],
  ["--on-accent", "--accent", 4.5],
  ["--danger", "--surface", 4.5],
  ["--border-strong", "--surface", 3],
];

describe("contrast", () => {
  test("follows WCAG 2.2's definition", () => {
    expect(contrast("#000000", "#ffffff")).toBeCloseTo(21, 12);
    expect(contrast("#ffffff", "#ffffff")).toBe(1);
    expect(contrast("#ffffff", "#000000")).toBeCloseTo(21, 12);
    // #767676 is the lightest grey that reaches 4.5:1 on white; #777777 misses it.
    expect(contrast("#767676", "#ffffff")).toBeGreaterThanOrEqual(4.5);
    expect(contrast("#777777", "#ffffff")).toBeLessThan(4.5);
  });

  test("every theme reaches 4.5:1 for text and 3:1 for control borders", () => {
    const all = themes(css);
    expect(all.length).toBeGreaterThanOrEqual(8); // light and dark, each with every accent
    for (const theme of all) {
      for (const [fg, bg, min] of PAIRS) {
        const [a, b] = [theme.tokens.get(fg), theme.tokens.get(bg)];
        expect(a && b, `${theme.name}: ${fg} and ${bg} are defined`).toBeTruthy();
        expect(contrast(a ?? "", b ?? ""), `${theme.name}: ${fg} on ${bg}`).toBeGreaterThanOrEqual(min);
      }
    }
  });
});

describe("luminance", () => {
  test("reads #rrggbb in either case and nothing else", () => {
    expect(luminance("#FFFFFF")).toBe(1);
    expect(luminance("#ffffff")).toBe(1);
    for (const bad of ["ffffff", "#fff", "#ffffff0", "x#ffffff", "#gggggg", ""]) {
      expect(luminance(bad), bad).toBeNaN();
    }
  });

  test("linearises dark channels by 1/12.92 and the rest by the power law", () => {
    // 10/255 sits below the sRGB threshold 0.04045, 11/255 above it; the two branches differ there in the fourth digit.
    expect(luminance("#0a0a0a")).toBeCloseTo(10 / 255 / 12.92, 9);
    expect(luminance("#0b0b0b")).toBeCloseTo(((11 / 255 + 0.055) / 1.055) ** 2.4, 9);
    expect(luminance("#ff0000")).toBeCloseTo(0.2126, 12);
    expect(luminance("#00ff00")).toBeCloseTo(0.7152, 12);
    expect(luminance("#0000ff")).toBeCloseTo(0.0722, 12);
  });
});

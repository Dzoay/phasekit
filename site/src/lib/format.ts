// Numbers as the page shows them: seven significant digits, scientific notation outside [10⁻³, 10⁷). The exact value
// stays in each cell's `data-value` and title.

const FIXED = new Intl.NumberFormat("en", { maximumSignificantDigits: 7, useGrouping: false });
const SUPERSCRIPT: Record<string, string> = {
  "-": "⁻",
  "0": "⁰",
  "1": "¹",
  "2": "²",
  "3": "³",
  "4": "⁴",
  "5": "⁵",
  "6": "⁶",
  "7": "⁷",
  "8": "⁸",
  "9": "⁹",
};

export function formatNumber(x: number): string {
  if (Number.isNaN(x)) return "—";
  if (!Number.isFinite(x)) return x > 0 ? "∞" : "−∞";
  const a = Math.abs(x);
  if (a === 0 || (a >= 1e-3 && a < 1e7)) return FIXED.format(x).replace("-", "−");
  const [mantissa = "", exponent = ""] = x.toExponential(6).split("e");
  const m = mantissa.replace(/\.?0+$/, "").replace("-", "−");
  const e = [...exponent.replace("+", "")].map((c) => SUPERSCRIPT[c] ?? c).join("");
  return `${m} × 10${e}`;
}

/** A number as an input field shows it: the shortest string that reads back as the same number. */
export function inputText(x: number): string {
  return Number.isFinite(x) ? String(x) : "";
}

// Numbers as the page shows them: seven significant digits, scientific notation outside [10⁻³, 10⁷), the same in every
// locale (a decimal point, no grouping). The exact value stays in each cell's `data-value` and title.

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

/** An integer as a superscript, for powers of ten. */
export function superscript(n: number): string {
  return [...String(n)].map((c) => SUPERSCRIPT[c] ?? c).join("");
}

export function formatNumber(x: number): string {
  if (Number.isNaN(x)) return "—";
  if (x === Number.POSITIVE_INFINITY) return "∞";
  if (x === Number.NEGATIVE_INFINITY) return "−∞";
  const a = Math.abs(x);
  if (a === 0 || (a >= 1e-3 && a < 1e7)) return String(Number(x.toPrecision(7))).replace("-", "−");
  const text = x.toExponential(6);
  const at = text.indexOf("e");
  const [mantissa, exponent] = [text.slice(0, at), text.slice(at + 1)];
  const m = mantissa.replace(/\.?0+$/, "").replace("-", "−");
  return `${m} × 10${superscript(Number(exponent))}`;
}

/** A duration given in µs: three significant digits, in µs while that rounds below a millisecond, else in ms. */
export function formatTime(us: number): string {
  return Number(us.toPrecision(3)) < 1000 ? `${us.toPrecision(3)} µs` : `${(us / 1000).toPrecision(3)} ms`;
}

/** A size given in bytes: in B below a kilobyte, else three significant digits in kB while that rounds below a
 *  megabyte, else in MB (SI prefixes: 10³ and 10⁶ bytes). */
export function formatBytes(bytes: number): string {
  if (bytes < 1000) return `${bytes} B`;
  const kB = (bytes / 1e3).toPrecision(3);
  return Number(kB) < 1000 ? `${kB} kB` : `${(bytes / 1e6).toPrecision(3)} MB`;
}

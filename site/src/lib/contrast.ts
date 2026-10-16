// WCAG 2.2's relative luminance and contrast ratio (its glossary, "relative luminance" and "contrast ratio"): the sRGB
// channels linearised, weighted 0.2126, 0.7152, 0.0722; the ratio (L1 + 0.05) / (L2 + 0.05), lighter over darker.

function linear(channel: number): number {
  const c = channel / 255;
  return c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
}

/** The relative luminance of a `#rrggbb` colour, or NaN for anything else. */
export function luminance(hex: string): number {
  if (!/^#[0-9a-f]{6}$/i.test(hex)) return Number.NaN;
  const n = Number.parseInt(hex.slice(1), 16);
  return 0.2126 * linear((n >> 16) & 255) + 0.7152 * linear((n >> 8) & 255) + 0.0722 * linear(n & 255);
}

/** The contrast ratio of two `#rrggbb` colours, from 1 to 21. */
export function contrast(a: string, b: string): number {
  const [la, lb] = [luminance(a), luminance(b)];
  return (Math.max(la, lb) + 0.05) / (Math.min(la, lb) + 0.05);
}

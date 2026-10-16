// The diagrams' snap grid (PLAN.md M21.8; the user's request): the values an axis snaps to, in the shown units, at one
// of five densities, so a viewer picks round values (1 bar, 2500 kJ/kg, 25 °C) with the cursor or the arrow keys.
import { scaleLinear } from "d3-scale";
import { formatNumber } from "../lib/format";

export const GRID_LEVELS = 5;

/** A linear axis's grid: d3's nice ticks at these counts, so steps of 1, 2 or 5 × 10ⁿ. */
const COUNTS = [5, 10, 20, 50, 100];

/** A log axis's grid: the mantissas per decade, in tenths, so that each value is an integer over a power of ten and
 *  reads as the round decimal it names (0.15, not 0.15000000000000002). */
const range = (from: number, to: number, by: number) =>
  Array.from({ length: Math.floor((to - from) / by) + 1 }, (_, i) => from + i * by);
const MANTISSAS = [[10], [10, 20, 50], range(10, 90, 10), range(10, 95, 5), range(10, 99, 1)];

const NAMES = ["decades", "1, 2, 5 per decade", "1 to 9 per decade", "halves per decade", "tenths per decade"];

/** A level, 1 to GRID_LEVELS, as an index. */
const index = (level: number) => Math.min(GRID_LEVELS, Math.max(1, Math.round(level))) - 1;

/** The grid's values within [a, b], ends included. */
export function gridValues([a, b]: readonly [number, number], log: boolean, level: number): number[] {
  if (!log) return scaleLinear().domain([a, b]).ticks(COUNTS[index(level)]);
  const values: number[] = [];
  for (let k = Math.floor(Math.log10(a)); k <= Math.ceil(Math.log10(b)); k++) {
    for (const m of MANTISSAS[index(level)] as number[]) {
      // Stryker disable next-line EqualityOperator: at k = 1 both forms are m itself.
      const v = k >= 1 ? m * 10 ** (k - 1) : m / 10 ** (1 - k);
      if (a <= v && v <= b) values.push(v);
    }
  }
  return values;
}

/** The grid value nearest `value`, in log space on a log axis; the lower one on a tie. */
export function snap(value: number, grid: readonly number[], log: boolean): number {
  const distance = (g: number) => (log ? Math.abs(Math.log10(g / value)) : Math.abs(g - value));
  return grid.reduce((best, g) => (distance(g) < distance(best) ? g : best), grid[0] ?? value);
}

/** The grid value one along from the one nearest `value` (as `snap` finds it), staying at the ends. */
export function step(value: number, grid: readonly number[], log: boolean, direction: 1 | -1): number {
  const at = grid.indexOf(snap(value, grid, log));
  return grid[Math.min(grid.length - 1, Math.max(0, at + direction))] ?? value;
}

/** How the grid reads: its step on a linear axis, its mantissas on a log one. */
export function describeGrid(domain: readonly [number, number], log: boolean, level: number): string {
  if (log) return NAMES[index(level)] as string;
  const [first = 0, second = 0] = gridValues(domain, false, level);
  return `every ${formatNumber(second - first)}`;
}

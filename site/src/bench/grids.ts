// The benchmark's states (PLAN.md M21.3): N states of one input pair over a fluid's plotted ranges, a near-square grid
// in the pair's two inputs, the last row cut short at N.
import { linGrid, logGrid, ranges, type Limits } from "../sample/ranges";

export type Pair = "DT" | "PT" | "QT" | "PQ";

export const PAIRS: readonly { id: Pair; label: string }[] = [
  { id: "DT", label: "ρ, T" },
  { id: "PT", label: "p, T" },
  { id: "QT", label: "q, T" },
  { id: "PQ", label: "p, q" },
];

/** The outputs each state is asked for: a flash's usual four. */
export const BENCH_OUTPUTS = ["Hmass", "Smass", "Cpmass", "speed_of_sound"] as const;

export interface Grid {
  name1: string;
  values1: Float64Array;
  name2: string;
  values2: Float64Array;
}

/** `n` states of `pair`: T over the plotted range (to just below T_c with a quality), ρ from ρ_c/1000 to 3 ρ_c, p over
 *  the plotted range (to just below p_c with a quality), q from 0 to 1. */
export function benchGrid(pair: Pair, l: Limits, n: number): Grid {
  const r = ranges(l);
  const side = Math.max(2, Math.ceil(Math.sqrt(n)));
  const below = 0.999;
  const [name1, a, name2, b]: [string, number[], string, number[]] =
    pair === "DT"
      ? ["T", linGrid(r.T[0], r.T[1], side), "Dmass", logGrid(l.rhoc / 1000, 3 * l.rhoc, side)]
      : pair === "PT"
        ? ["T", linGrid(r.T[0], r.T[1], side), "P", logGrid(r.P[0], r.P[1], side)]
        : pair === "QT"
          ? ["T", linGrid(r.T[0], l.Tc * below, side), "Q", linGrid(0, 1, side)]
          : ["P", logGrid(r.P[0], l.pc * below, side), "Q", linGrid(0, 1, side)];
  const values1 = new Float64Array(n);
  const values2 = new Float64Array(n);
  // Stryker disable next-line EqualityOperator: a typed array ignores a write past its end.
  for (let i = 0; i < n; i++) {
    values1[i] = a[i % side] as number;
    values2[i] = b[Math.floor(i / side)] as number;
  }
  return { name1, values1, name2, values2 };
}

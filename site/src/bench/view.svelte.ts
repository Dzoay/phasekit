// The benchmark page's view (PLAN.md M21.3): a fluid, an input pair and a number of states, in the URL
// (`fluid=n-Propane&pair=DT&n=10000`); a run times the package's batch on each build this browser runs, one after the
// other, counts the cells by status and compares the builds' answers bit for bit (user decision SL8). A run starts
// only when asked; its results stay with the settings they ran on. tests/unit/bench-view.svelte.test.ts drives it.
import { untrack } from "svelte";
import type { BatchValue, Build, Engine, Timing } from "../engine/protocol";
import { LIMIT_KEYS, limits } from "../lib/ranges";
import { byStatus, compareBuilds, type Comparison } from "./compare";
import { BENCH_OUTPUTS, PAIRS, benchGrid, type Pair } from "./grids";

/** The numbers of states a run offers. */
export const SIZES = [1_000, 10_000, 100_000] as const;

const DEFAULT = { fluid: "n-Propane", pair: "DT", n: 10_000 } as const;

const message = (e: unknown): string => (e instanceof Error ? e.message : String(e));

export interface BuildResult {
  build: Build;
  timing: Timing;
  /** States a second: the states of a run over its mean time. */
  perSecond: number;
  /** The cells by status code, the codes in ascending order. */
  counts: [number, number][];
}

export interface Run {
  /** The settings it ran on: `fluid/pair/n`. */
  key: string;
  results: BuildResult[];
  /** The builds' answers compared, where this browser runs both. */
  comparison: Comparison | null;
  refusal: string | null;
}

export class BenchView {
  fluid = $state<string>(DEFAULT.fluid);
  pair = $state<Pair>(DEFAULT.pair);
  n = $state<number>(DEFAULT.n);
  running = $state(false);
  run = $state.raw<Run | null>(null);

  readonly #engines: readonly { build: Build; engine: Engine }[];
  #sequence = 0;

  /** `engines`: one for each build this browser runs, the baseline first. */
  constructor(engines: readonly { build: Build; engine: Engine }[]) {
    this.#engines = engines;
  }

  /** The settings a run would use now. */
  key(): string {
    return `${this.fluid}/${this.pair}/${this.n}`;
  }

  /** The view a URL's params name; anything malformed falls back to 10 000 states of n-Propane by ρ and T. */
  load(params: URLSearchParams): void {
    this.fluid = params.get("fluid") ?? DEFAULT.fluid;
    this.pair = PAIRS.find((p) => p.id === params.get("pair"))?.id ?? DEFAULT.pair;
    this.n = SIZES.find((n) => String(n) === params.get("n")) ?? DEFAULT.n;
  }

  params(): URLSearchParams {
    return new URLSearchParams({ fluid: this.fluid, pair: this.pair, n: String(this.n) });
  }

  connect(route: { readonly params: URLSearchParams }, write: (params: URLSearchParams) => void): () => void {
    return $effect.root(() => {
      $effect(() => {
        const params = route.params;
        untrack(() => this.load(params));
      });
      $effect(() => write(this.params()));
    });
  }

  /** Time the batch on each build, then compare their answers; a run started meanwhile drops this one's results. */
  async start(): Promise<void> {
    // Stryker disable next-line UpdateOperator: as the diagrams view's.
    const mine = ++this.#sequence;
    const [key, fluid, pair, n] = [this.key(), this.fluid, this.pair, this.n];
    this.running = true;
    let run: Run = { key, results: [], comparison: null, refusal: null };
    const show = (change: Partial<Run>) => (this.run = run = { ...run, ...change });
    show({});
    const answers: BatchValue[] = [];
    try {
      const first = this.#engines[0] as { engine: Engine };
      const l = limits(await first.engine.call({ op: "constants", fluid, keys: [...LIMIT_KEYS] }));
      if (!l) throw new Error(`${fluid} names no critical or triple point to place its states by`);
      const grid = benchGrid(pair, l, n);
      for (const { build, engine } of this.#engines) {
        const value = await engine.call({ op: "bench", fluid, outputs: [...BENCH_OUTPUTS], ...grid });
        if (mine !== this.#sequence) return;
        answers.push(value.batch);
        const counts = [...byStatus(value.batch.status)].sort(([a], [b]) => a - b);
        const result = { build, timing: value.timing, perSecond: n / (value.timing.us * 1e-6), counts };
        show({ results: [...run.results, result] });
      }
      const [a, b] = answers;
      if (a && b) show({ comparison: compareBuilds(a, b) });
    } catch (e) {
      if (mine === this.#sequence) show({ refusal: message(e) });
    } finally {
      if (mine === this.#sequence) untrack(() => (this.running = false));
    }
  }
}

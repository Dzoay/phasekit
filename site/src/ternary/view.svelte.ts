// The ternary page's view (PLAN.md M26.6): a three-component mixture and a T and p in the URL
// (`mixture=R407C.mix&T=273.15&p=500000`); the mixtures the page offers, the predefined ones of three components; and
// the diagram, the package's, asked once for each mixture, T and p and kept; a refusal in the package's words. The
// component binds to it; tests/unit/ternary-view.svelte.test.ts drives it.
import { untrack } from "svelte";
import type { Engine, Refusal } from "../engine/protocol";
import { refusalOf } from "../lib/refusal";
import { readTernary, type TernaryData } from "./geometry";

const DEFAULT = { mixture: "R407C.mix", T: 273.15, p: 5e5 } as const;

/** A positive finite number from the URL, or the default. */
function positive(raw: string | null, fallback: number): number {
  const v = Number(raw);
  return raw !== null && raw.trim() !== "" && Number.isFinite(v) && v > 0 ? v : fallback;
}

export class TernaryView {
  mixture = $state<string>(DEFAULT.mixture);
  T = $state<number>(DEFAULT.T);
  p = $state<number>(DEFAULT.p);
  /** The predefined mixtures of three components, once the engine lists them. */
  choices = $state.raw<string[]>([]);
  data = $state.raw<TernaryData | null>(null);
  refusal = $state<Refusal | null>(null);
  busy = $state(true);

  readonly #engine: Engine;
  readonly #cache = new Map<string, TernaryData>();
  #sequence = 0;

  constructor(engine: Engine) {
    this.#engine = engine;
  }

  /** The view a URL's params name: R407C at 273.15 K and 5 bar for anything missing or malformed. */
  load(params: URLSearchParams): void {
    this.mixture = params.get("mixture") ?? DEFAULT.mixture;
    this.T = positive(params.get("T"), DEFAULT.T);
    this.p = positive(params.get("p"), DEFAULT.p);
  }

  params(): URLSearchParams {
    return new URLSearchParams({ mixture: this.mixture, T: String(this.T), p: String(this.p) });
  }

  /** A new temperature, K, or pressure, Pa; anything not positive and finite is ignored. */
  set(key: "T" | "p", value: number): void {
    if (Number.isFinite(value) && value > 0) this[key] = value;
  }

  /** The wiring: the route loads the view, the view writes the URL, the mixtures are listed and a mixture, T and p
   *  load their diagram. Returns the function that disconnects it. */
  connect(route: { readonly params: URLSearchParams }, write: (params: URLSearchParams) => void): () => void {
    this.#engine.call({ op: "mixtures" }).then(
      (m) => (this.choices = m.predefined.filter((x) => x.components.length === 3).map((x) => x.name)),
      () => undefined,
    );
    return $effect.root(() => {
      $effect(() => {
        const params = route.params;
        untrack(() => this.load(params));
      });
      $effect(() => write(this.params()));
      $effect(() => this.#load(this.mixture, this.T, this.p));
    });
  }

  // The diagram: from the cache, or the package's; an answer for a mixture, T or p no longer chosen is dropped.
  #load(mixture: string, T: number, p: number): void {
    // Stryker disable next-line UpdateOperator: as SurfaceView's.
    const mine = ++this.#sequence;
    const key = `${mixture}/${T}/${p}`;
    const cached = this.#cache.get(key);
    if (cached) {
      [this.data, this.refusal, this.busy] = [cached, null, false];
      return;
    }
    untrack(() => (this.busy = true));
    this.#engine.call({ op: "ternary", fluid: mixture, T, p }).then(
      (value) => {
        const data = readTernary(value);
        this.#cache.set(key, data);
        if (mine === this.#sequence) [this.data, this.refusal, this.busy] = [data, null, false];
      },
      (e: unknown) => {
        if (mine === this.#sequence) [this.data, this.refusal, this.busy] = [null, refusalOf(e), false];
      },
    );
  }
}

// The curves page's view (PLAN.md M26.6; user decision DG10): a fluid in the URL (`fluid=Nitrogen`); its
// characteristic curves and compressibility chart, the package's, and its saturation line on p–T to draw them against,
// both asked once per fluid and kept; a refusal in the package's words, a mixture's among them. The component binds to
// it; tests/unit/curves-view.svelte.test.ts drives it.
import { untrack } from "svelte";
import type { Engine, Refusal } from "../engine/protocol";
import { readDiagram, type Vertex } from "../diagram/data";
import { readCurves, type CurvesData } from "../diagram/overlays";
import { refusalOf } from "../lib/refusal";

const DEFAULT_FLUID = "Nitrogen";

/** A fluid's curves and, on p–T, its saturation line, T across and p up, and its critical point. */
export interface CurvesPage {
  curves: CurvesData;
  saturation: Vertex[];
  critical: Vertex | null;
}

export class CurvesView {
  fluid = $state<string>(DEFAULT_FLUID);
  data = $state.raw<CurvesPage | null>(null);
  refusal = $state<Refusal | null>(null);
  busy = $state(true);

  readonly #engine: Engine;
  readonly #cache = new Map<string, CurvesPage>();
  #sequence = 0;

  constructor(engine: Engine) {
    this.#engine = engine;
  }

  /** The view a URL's params name: Nitrogen's curves for anything missing. */
  load(params: URLSearchParams): void {
    this.fluid = params.get("fluid") ?? DEFAULT_FLUID;
  }

  params(): URLSearchParams {
    return new URLSearchParams({ fluid: this.fluid });
  }

  /** The wiring: the route loads the view, the view writes the URL, and a fluid loads its curves. Returns the function
   *  that disconnects it. */
  connect(route: { readonly params: URLSearchParams }, write: (params: URLSearchParams) => void): () => void {
    return $effect.root(() => {
      $effect(() => {
        const params = route.params;
        untrack(() => this.load(params));
      });
      $effect(() => write(this.params()));
      $effect(() => this.#load(this.fluid));
    });
  }

  // The fluid's curves and saturation line: from the cache, or the package's, two calls; an answer for a fluid no
  // longer chosen is dropped.
  #load(fluid: string): void {
    // Stryker disable next-line UpdateOperator: as SurfaceView's.
    const mine = ++this.#sequence;
    const cached = this.#cache.get(fluid);
    if (cached) {
      [this.data, this.refusal, this.busy] = [cached, null, false];
      return;
    }
    untrack(() => (this.busy = true));
    Promise.all([
      this.#engine.call({ op: "curves", fluid }),
      this.#engine.call({ op: "diagram", fluid, plane: "pt" }),
    ]).then(
      ([curves, diagram]) => {
        const { liquid, critical } = readDiagram(diagram).dome;
        const data = { curves: readCurves(curves), saturation: liquid, critical };
        this.#cache.set(fluid, data);
        if (mine === this.#sequence) [this.data, this.refusal, this.busy] = [data, null, false];
      },
      (e: unknown) => {
        if (mine === this.#sequence) [this.data, this.refusal, this.busy] = [null, refusalOf(e), false];
      },
    );
  }
}

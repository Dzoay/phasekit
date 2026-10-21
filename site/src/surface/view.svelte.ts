// The surfaces page's view (PLAN.md M26.2): a fluid and a surface, p–v–T or u–s–v, in the URL (`fluid=Water&kind=usv`);
// the fluid's mesh, the package's, asked once per fluid and kept; a refusal in the package's words, a mixture's among
// them. The component binds to it; tests/unit/surface-view.svelte.test.ts drives it.
import { untrack } from "svelte";
import type { Engine, SurfaceValue } from "../engine/protocol";
import { kindOf, type SurfaceKind } from "./mesh";

const DEFAULT_FLUID = "Water";

const message = (e: unknown): string => (e instanceof Error ? e.message : String(e));

export class SurfaceView {
  fluid = $state<string>(DEFAULT_FLUID);
  kind = $state<SurfaceKind>("pvt");
  // The mesh is replaced whole, never changed in place: raw state, which proxies none of its arrays.
  data = $state.raw<SurfaceValue | null>(null);
  refusal = $state<string | null>(null);
  busy = $state(true);

  readonly #engine: Engine;
  readonly #cache = new Map<string, SurfaceValue>();
  #sequence = 0;

  constructor(engine: Engine) {
    this.#engine = engine;
  }

  /** The view a URL's params name: Water's p–v–T surface for anything missing or malformed. */
  load(params: URLSearchParams): void {
    this.fluid = params.get("fluid") ?? DEFAULT_FLUID;
    this.kind = kindOf(params.get("kind"));
  }

  params(): URLSearchParams {
    const params = new URLSearchParams({ fluid: this.fluid });
    if (this.kind !== "pvt") params.set("kind", this.kind);
    return params;
  }

  choose(kind: SurfaceKind): void {
    this.kind = kind;
  }

  /** The wiring: the route loads the view, the view writes the URL, and a fluid loads its mesh. Returns the function
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

  // The fluid's mesh: from the cache, or the package's; an answer for a fluid no longer chosen is dropped.
  #load(fluid: string): void {
    // Stryker disable next-line UpdateOperator: as DiagramView's.
    const mine = ++this.#sequence;
    const cached = this.#cache.get(fluid);
    if (cached) {
      [this.data, this.refusal, this.busy] = [cached, null, false];
      return;
    }
    this.busy = true;
    this.#engine.call({ op: "surface", fluid }).then(
      (data) => {
        this.#cache.set(fluid, data);
        if (mine === this.#sequence) [this.data, this.refusal, this.busy] = [data, null, false];
      },
      (e: unknown) => {
        if (mine === this.#sequence) [this.data, this.refusal, this.busy] = [null, message(e), false];
      },
    );
  }
}

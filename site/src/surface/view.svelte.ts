// The surfaces page's view (PLAN.md M26.2): a fluid and a surface, p–v–T or u–s–v, in the URL (`fluid=Water&kind=usv`);
// the fluid's mesh, the package's, asked once per fluid and kept; a refusal in the package's words, a mixture's among
// them. The component binds to it; tests/unit/surface-view.svelte.test.ts drives it.
import { untrack } from "svelte";
import type { Engine, StateValue, SurfaceValue } from "../engine/protocol";
import { kindOf, type SurfaceKind } from "./mesh";

const DEFAULT_FLUID = "Water";

/** What the readout shows of the state under the cursor, beside its phase. */
export const READOUT_KEYS = ["T", "P", "Dmass", "Umass", "Hmass", "Smass", "Q"] as const;

/** The state under the cursor: its (T, ρ) on the surface, and the package's state there or its refusal. */
export interface SurfaceReadout {
  inputs: [number, number];
  state: StateValue | null;
  refusal: string | null;
}

const message = (e: unknown): string => (e instanceof Error ? e.message : String(e));

export class SurfaceView {
  fluid = $state<string>(DEFAULT_FLUID);
  kind = $state<SurfaceKind>("pvt");
  // The mesh is replaced whole, never changed in place: raw state, which proxies none of its arrays.
  data = $state.raw<SurfaceValue | null>(null);
  refusal = $state<string | null>(null);
  busy = $state(true);
  readout = $state.raw<SurfaceReadout | null>(null);

  readonly #engine: Engine;
  readonly #cache = new Map<string, SurfaceValue>();
  #sequence = 0;
  #pointer = 0;

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

  /** The package's state at a point of the surface, (T, ρ); only the answer for the latest point is shown. */
  hover(t: number, rho: number): void {
    // Stryker disable next-line UpdateOperator: as the sequence's.
    const mine = ++this.#pointer;
    const inputs: [number, number] = [t, rho];
    const shown = (readout: SurfaceReadout) => {
      if (mine === this.#pointer) this.readout = readout;
    };
    const request = { op: "state" as const, fluid: this.fluid, name1: "T", value1: t, name2: "Dmass", value2: rho };
    this.#engine.call({ ...request, outputs: [...READOUT_KEYS] }).then(
      (state) => shown({ inputs, state, refusal: null }),
      (e: unknown) => shown({ inputs, state: null, refusal: message(e) }),
    );
  }

  /** The cursor has left the surface: no readout, and none that arrives late. */
  leave(): void {
    this.#pointer += 1;
    this.readout = null;
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
    this.leave();
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

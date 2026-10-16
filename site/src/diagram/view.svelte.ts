// The diagrams page's view (PLAN.md M21.2): a fluid, a diagram, the units and the families turned off, in the URL
// (`fluid=Water&d=ph&units=engineering&hide=T,Q`); the diagram's geometry, sampled once per fluid and diagram; and the
// readout, the package's state at the cursor. The component binds to it; tests/unit/view.svelte.test.ts drives it.
import { untrack } from "svelte";
import type { Engine, StateValue } from "../engine/protocol";
import type { System } from "../lib/units";
import { diagram, type Diagram, type DiagramId, type Key } from "../sample/diagrams";
import { sampleDiagram, type Batch, type DiagramData } from "../sample/lines";
import { LIMIT_KEYS, limits } from "../sample/ranges";
import { GRID_LEVELS } from "./grid";

/** What the readout shows of the state under the cursor, beside its phase. */
export const READOUT_KEYS = ["T", "P", "Hmass", "Smass", "Dmass", "Q"] as const;

const FAMILIES: readonly Key[] = ["T", "P", "Hmass", "Smass", "Dmass", "Q"];

const DEFAULT = { fluid: "Water", id: "ph", grid: 3 } as const;

/** A grid level from the URL: a number rounded into 1..GRID_LEVELS, the default for anything else. */
function gridLevel(raw: string | null): number {
  const level = Number(raw);
  return raw === null || raw.trim() === "" || !Number.isFinite(level)
    ? DEFAULT.grid
    : Math.min(GRID_LEVELS, Math.max(1, Math.round(level)));
}

export interface Readout {
  inputs: [Key, number, Key, number];
  state: StateValue | null;
  refusal: string | null;
}

const message = (e: unknown): string => (e instanceof Error ? e.message : String(e));

export class DiagramView {
  fluid = $state<string>(DEFAULT.fluid);
  id = $state<DiagramId>(DEFAULT.id);
  system = $state<System>("si");
  hidden = $state<Key[]>([]);
  /** Whether the cursor snaps to the grid, and the grid's density, 1 to GRID_LEVELS (PLAN.md M21.8). */
  snap = $state(false);
  grid = $state<number>(DEFAULT.grid);
  /** A pinned readout stays where it was clicked; only the arrow keys' steps move it. */
  pinned = $state(false);

  // The geometry and the readout are replaced whole, never changed in place: raw state, which proxies neither its
  // thousands of vertices nor the cache's copies, so a cached diagram is the same object again.
  data = $state.raw<DiagramData | null>(null);
  refusal = $state<string | null>(null);
  busy = $state(true);
  readout = $state.raw<Readout | null>(null);

  readonly diagram: Diagram = $derived(diagram(this.id) as Diagram);

  readonly #engine: Engine;
  readonly #cache = new Map<string, DiagramData>();
  #sequence = 0;
  #pointer = 0;
  #flying = false;
  #latest: [Key, number, Key, number] | null = null;

  constructor(engine: Engine) {
    this.#engine = engine;
  }

  /** The view a URL's params name; anything malformed falls back to Water's p–h diagram in SI. */
  load(params: URLSearchParams): void {
    this.fluid = params.get("fluid") ?? DEFAULT.fluid;
    this.id = diagram(String(params.get("d")))?.id ?? DEFAULT.id;
    this.system = params.get("units") === "engineering" ? "engineering" : "si";
    this.snap = params.get("snap") === "1";
    this.grid = gridLevel(params.get("grid"));
    this.hidden = String(params.get("hide"))
      .split(",")
      .filter((k): k is Key => FAMILIES.includes(k as Key));
  }

  params(): URLSearchParams {
    const params = new URLSearchParams({ fluid: this.fluid, d: this.id });
    if (this.system === "engineering") params.set("units", this.system);
    if (this.hidden.length > 0) params.set("hide", this.hidden.join(","));
    if (this.snap) params.set("snap", "1");
    if (this.grid !== DEFAULT.grid) params.set("grid", String(this.grid));
    return params;
  }

  toggle(held: Key): void {
    this.hidden = this.hidden.includes(held) ? this.hidden.filter((k) => k !== held) : [...this.hidden, held];
  }

  setSystem(system: System): void {
    this.system = system;
  }

  choose(id: DiagramId): void {
    this.id = id;
  }

  /** The state at a pixel's inputs, from the engine. One flash is in flight at a time: points that come meanwhile
   *  wait in one place, the latest replacing the rest, and only an answer for the latest point is shown. */
  point(inputs: [Key, number, Key, number]): void {
    this.#pointer += 1;
    this.#latest = inputs;
    if (!this.#flying) this.#flash();
  }

  toggleSnap(): void {
    this.snap = !this.snap;
  }

  setGrid(level: number): void {
    this.grid = gridLevel(String(level));
  }

  /** The cursor's point: the readout's, unless it is pinned. */
  hover(inputs: [Key, number, Key, number]): void {
    if (!this.pinned) this.point(inputs);
  }

  togglePin(): void {
    this.pinned = !this.pinned;
  }

  unpin(): void {
    this.pinned = false;
  }

  /** The calculator at the readout's state: its two inputs, the fluid and the units. */
  calculatorLink(): string | null {
    const inputs = this.readout?.inputs;
    if (!inputs) return null;
    const [k1, v1, k2, v2] = inputs;
    const params = new URLSearchParams({ fluid: this.fluid, a: `${k1}:${v1}`, b: `${k2}:${v2}` });
    if (this.system === "engineering") params.set("units", this.system);
    return `#/calculator?${params}`;
  }

  /** The cursor has left the plot: no readout, none that arrives late, and no waiting point; unless it is pinned. */
  leave(): void {
    if (this.pinned) return;
    this.#pointer += 1;
    this.#latest = null;
    this.readout = null;
  }

  #flash(): void {
    const inputs = this.#latest;
    if (!inputs) return;
    [this.#latest, this.#flying] = [null, true];
    const mine = this.#pointer;
    const done = (readout: Readout) => {
      this.#flying = false;
      if (mine === this.#pointer) this.readout = readout;
      this.#flash();
    };
    const [name1, value1, name2, value2] = inputs;
    const request = { op: "state" as const, fluid: this.fluid, name1, value1, name2, value2 };
    this.#engine.call({ ...request, outputs: [...READOUT_KEYS] }).then(
      (state) => done({ inputs, state, refusal: null }),
      (e: unknown) => done({ inputs, state: null, refusal: message(e) }),
    );
  }

  /** The wiring: the route loads the view, the view writes the URL, and a fluid and diagram load their geometry.
   *  Returns the function that disconnects it. */
  connect(route: { readonly params: URLSearchParams }, write: (params: URLSearchParams) => void): () => void {
    return $effect.root(() => {
      $effect(() => {
        const params = route.params;
        untrack(() => this.load(params));
      });
      $effect(() => write(this.params()));
      $effect(() => this.#sample(this.fluid, this.id));
    });
  }

  // The diagram's geometry: from the cache, or the fluid's limits and then its sampling; a late answer is dropped.
  #sample(fluid: string, id: DiagramId): void {
    const key = `${fluid}/${id}`;
    // Stryker disable next-line UpdateOperator: as the pointer's.
    const mine = ++this.#sequence;
    const cached = this.#cache.get(key);
    if (cached) {
      [this.data, this.refusal, this.busy] = [cached, null, false];
      return;
    }
    untrack(() => (this.busy = true));
    const batch: Batch = (outputs, name1, values1, name2, values2) =>
      this.#engine.call({ op: "batch", fluid, outputs, name1, values1, name2, values2 });
    this.#engine
      .call({ op: "constants", fluid, keys: [...LIMIT_KEYS] })
      .then((values) => {
        const l = limits(values);
        if (!l) throw new Error(`${fluid} names no critical or triple point to draw a dome from`);
        return sampleDiagram(diagram(id) as Diagram, l, batch);
      })
      .then(
        (data) => {
          this.#cache.set(key, data);
          if (mine === this.#sequence) [this.data, this.refusal, this.busy] = [data, null, false];
        },
        (e: unknown) => {
          if (mine === this.#sequence) [this.data, this.refusal, this.busy] = [null, message(e), false];
        },
      );
  }
}

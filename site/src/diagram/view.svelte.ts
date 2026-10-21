// The diagrams page's view (PLAN.md M21.2): a fluid, a diagram, the units and the families turned off, in the URL
// (`fluid=Water&d=ph&units=engineering&hide=T,Q`); the diagram's geometry, the package's (M21.6), asked once per fluid
// and diagram, a mixture's its phase envelope on p–T or T–ρ, its only planes (M21.5); the phase regions while they are
// shown (`regions=1`, M21.9), asked once per fluid; and the readout, the package's state at the cursor. The component
// binds to it; tests/unit/view.svelte.test.ts drives it.
import { untrack } from "svelte";
import type { Engine, Refusal, StateValue } from "../engine/protocol";
import { refusalOf } from "../lib/refusal";
import type { System } from "../lib/units";
import { readDiagram, readRegions, type DiagramData, type EnvelopeInfo, type Regions } from "./data";
import { GRID_LEVELS } from "./grid";
import { MIXTURE_DIAGRAMS, diagram, type Diagram, type DiagramId, type Key } from "./planes";

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
  /** Why the package refused the point, which the readout writes short, in the shown units (PLAN.md M28.3). */
  refusal: Refusal | null;
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
  /** Whether the plot is tinted by phase (PLAN.md M21.9), and the fluid's regions once they are sampled. */
  regions = $state(false);
  phases = $state.raw<Regions | null>(null);

  // The geometry and the readout are replaced whole, never changed in place: raw state, which proxies neither its
  // thousands of vertices nor the cache's copies, so a cached diagram is the same object again.
  data = $state.raw<DiagramData | null>(null);
  /** The last geometry's mixture: its components and how its envelope's trace ended; null for a pure fluid's. */
  mixture = $state.raw<EnvelopeInfo | null>(null);
  refusal = $state<string | null>(null);
  busy = $state(true);
  readout = $state.raw<Readout | null>(null);

  readonly diagram: Diagram = $derived(diagram(this.id) as Diagram);

  readonly #engine: Engine;
  readonly #cache = new Map<string, DiagramData>();
  readonly #regionCache = new Map<string, Regions>();
  readonly #components = new Map<string, Promise<string[]>>();
  #sequence = 0;
  #regionSequence = 0;
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
    this.regions = params.get("regions") === "1";
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
    if (this.regions) params.set("regions", "1");
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

  toggleRegions(): void {
    this.regions = !this.regions;
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
      (e: unknown) => done({ inputs, state: null, refusal: refusalOf(e) }),
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
      $effect(() => this.#load(this.fluid, this.id));
      $effect(() => this.#loadRegions(this.fluid, this.regions));
    });
  }

  // The diagram's geometry: from the cache, or the package's; a late answer is dropped, and so is a mixture's on a plane
  // it has not, which switches to p–T instead.
  #load(fluid: string, id: DiagramId): void {
    const key = `${fluid}/${id}`;
    // Stryker disable next-line UpdateOperator: as the pointer's.
    const mine = ++this.#sequence;
    const cached = this.#cache.get(key);
    if (cached) {
      [this.data, this.mixture, this.refusal, this.busy] = [cached, cached.envelope ?? null, null, false];
      return;
    }
    untrack(() => (this.busy = true));
    this.#geometry(fluid, id).then(
      (data) => {
        // A switch's null is late, the switch having started p–T's sample, so it could reach only the cache, where a
        // null entry is a miss.
        // Stryker disable next-line ConditionalExpression: equivalent, as above.
        if (!data) return;
        this.#cache.set(key, data);
        if (mine === this.#sequence)
          [this.data, this.mixture, this.refusal, this.busy] = [data, data.envelope ?? null, null, false];
      },
      (e: unknown) => {
        if (mine === this.#sequence)
          [this.data, this.mixture, this.refusal, this.busy] = [null, null, message(e), false];
      },
    );
  }

  // The package's diagram, from one call; or null where the package refused a mixture a plane it has not and the view
  // has switched to p–T. Only then is the engine asked whether the fluid is a mixture.
  async #geometry(fluid: string, id: DiagramId): Promise<DiagramData | null> {
    try {
      return readDiagram(await this.#engine.call({ op: "diagram", fluid, plane: id }));
    } catch (e) {
      if (MIXTURE_DIAGRAMS.includes(id) || (await this.#mixed(fluid)).length === 0) throw e;
      this.id = "pt";
      return null;
    }
  }

  // A mixture name's components, as the engine names them, asked once a name; none for a pure fluid, or a name the
  // engine refuses.
  #mixed(fluid: string): Promise<string[]> {
    let parts = this.#components.get(fluid);
    if (!parts) {
      parts = this.#engine.call({ op: "components", name: fluid }).catch(() => []);
      this.#components.set(fluid, parts);
    }
    return parts;
  }

  // The fluid's phase regions while they are shown: from the cache, or the package's, from one call; a late answer is
  // dropped, and a refused one shows none.
  #loadRegions(fluid: string, shown: boolean): void {
    // Stryker disable next-line UpdateOperator: as the pointer's.
    const mine = ++this.#regionSequence;
    const cached = this.#regionCache.get(fluid);
    if (!shown || cached) {
      this.phases = shown ? (cached as Regions) : null;
      return;
    }
    untrack(() => (this.phases = null));
    this.#engine.call({ op: "regions", fluid }).then(
      (value) => {
        const regions = readRegions(value);
        this.#regionCache.set(fluid, regions);
        if (mine === this.#regionSequence) this.phases = regions;
      },
      () => undefined,
    );
  }
}

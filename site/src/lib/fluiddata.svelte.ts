// What the page knows of its fluids (PLAN.md M21.4): packs/index.json, fetched once; each fluid's packs, fetched on the
// fluid's first use and added to the engine (src/engine/client.ts adds each to every worker, those that start later
// too); and, for the page to show, the packs fetched and their bytes, and what the registry serves and has decoded.
import type { RegistryValue } from "../engine/protocol";
import { formatBytes } from "./format";
import { find, plan, type PackEntry, type PackIndex } from "./packs";

/** Where the packs come from: packs/index.json and the files beside it. */
export interface Source {
  index(): Promise<PackIndex>;
  pack(file: string): Promise<Uint8Array>;
}

/** The engine's registry, as the store reaches it. */
export interface Registry {
  /** Adds a pack; the answer is what the registry serves and has decoded after it. */
  add(bytes: Uint8Array): Promise<RegistryValue>;
  /** What the registry serves and has decoded now. */
  contents(): Promise<RegistryValue>;
}

export class FluidData {
  /** packs/index.json's fluids, once it has come; none before. */
  fluids = $state.raw<readonly PackEntry[]>([]);
  /** The canonical name of every fluid the registry serves, once it has answered. */
  names = $state.raw<readonly string[]>([]);
  /** The fluids the registry has decoded. */
  loaded = $state.raw<readonly string[]>([]);
  /** The packs fetched, in the order they were added. */
  fetched = $state.raw<readonly PackEntry[]>([]);
  /** Why packs/index.json could not be fetched, while it could not. */
  error = $state<string | null>(null);
  /** The bytes of the packs fetched. */
  readonly bytes = $derived(this.fetched.reduce((sum, f) => sum + f.bytes, 0));
  /** What fetching every pack would cost. */
  readonly total = $derived(this.fluids.reduce((sum, f) => sum + f.bytes, 0));
  /** What the page has fetched and the registry decoded, in a sentence: ARCHITECTURE.md §8's "only what is needed". */
  readonly summary = $derived(
    `This page has fetched ${this.fetched.length} ${this.fetched.length === 1 ? "pack" : "packs"}, ` +
      `${formatBytes(this.bytes)} of ${formatBytes(this.total)}, and decoded ` +
      `${this.loaded.length === 0 ? "no fluid" : this.loaded.join(", ")}.`,
  );

  readonly #source: Source;
  readonly #registry: Registry;
  #index: Promise<readonly PackEntry[]> | undefined;
  #named: Promise<void> | undefined;
  readonly #adding = new Map<string, Promise<void>>();
  #refreshing: Promise<void> | undefined;
  #again = false;

  constructor(source: Source, registry: Registry) {
    this.#source = source;
    this.#registry = registry;
  }

  /** packs/index.json's fluids, fetched on the first call; a failure is kept in `error`, and the next call tries again. */
  index(): Promise<readonly PackEntry[]> {
    this.#index ??= this.#source.index().then(
      (index) => {
        this.error = null;
        return (this.fluids = index.fluids);
      },
      (e: unknown) => {
        this.#index = undefined;
        this.error = `the fluid index could not be fetched: ${e instanceof Error ? e.message : String(e)}`;
        throw e;
      },
    );
    return this.#index;
  }

  /** Adds the packs `fluid` needs, in order: none when the registry serves it already, or when the index does not know
   *  it (or could not be fetched), and the engine then answers for it. A pack that could not be fetched or added
   *  refuses the call, and the next call tries it again. */
  async ensure(fluid: string): Promise<void> {
    await (this.#named ??= this.refresh());
    if (this.names.some((n) => n.toLowerCase() === fluid.toLowerCase())) return;
    const fluids = await this.index().catch(() => []);
    for (const entry of plan(fluids, fluid, new Set(this.names))) await this.#add(entry);
  }

  /** Where an entry's data comes from: the package (the registry served it before any pack), its pack fetched, or its
   *  pack's size, to fetch on first use. */
  cost(entry: PackEntry): string {
    if (this.fetched.includes(entry)) return "fetched";
    return this.names.includes(entry.name) ? "embedded" : formatBytes(entry.bytes);
  }

  /** The chosen fluid's data, in a sentence: where it comes from, and where each fluid it requires does. Empty for a
   *  name neither the index nor the registry knows. */
  origin(fluid: string): string {
    const entry = find(this.fluids, fluid);
    if (!entry) return this.names.includes(fluid) ? `${fluid}: embedded in the package.` : "";
    const required = entry.requires.flatMap((r) => {
      const e = find(this.fluids, r);
      return e ? [`${e.name}, ${this.#from(e)}`] : [];
    });
    return `${entry.name}: ${this.#from(entry)}${required.length > 0 ? `; its transport reads ${required.join("; ")}` : ""}.`;
  }

  /** Asks the registry what it serves and has decoded. */
  async refresh(): Promise<void> {
    this.#show(await this.#registry.contents());
  }

  /** The registry may have decoded more: asks it again, once more after a request already in flight. */
  touch(): void {
    if (this.#refreshing) {
      this.#again = true;
      return;
    }
    this.#refreshing = this.refresh()
      .catch(() => {})
      .finally(() => {
        this.#refreshing = undefined;
        if (this.#again) {
          this.#again = false;
          this.touch();
        }
      });
  }

  // One pack, fetched and added once, however many fluids ask for it at once.
  #add(entry: PackEntry): Promise<void> {
    let adding = this.#adding.get(entry.pack);
    if (!adding) {
      adding = this.#source
        .pack(entry.pack)
        .then((bytes) => this.#registry.add(bytes))
        .then((contents) => {
          this.#show(contents);
          this.fetched = [...this.fetched, entry];
        });
      this.#adding.set(entry.pack, adding);
      adding.catch(() => this.#adding.delete(entry.pack));
    }
    return adding;
  }

  #from(entry: PackEntry): string {
    if (this.fetched.includes(entry)) return `fetched, ${formatBytes(entry.bytes)}`;
    return this.names.includes(entry.name) ? "embedded in the package" : `${formatBytes(entry.bytes)} to fetch`;
  }

  #show(contents: RegistryValue): void {
    this.names = contents.names;
    this.loaded = contents.loaded;
  }
}

// The fluid picker's view (PLAN.md M21.4): a combobox over every fluid of packs/index.json, searched by name, alias or
// CAS number (src/lib/packs.ts), or, until the index has come, over the fluids the registry serves. Its value is the
// chosen fluid's canonical name. FluidPicker.svelte binds to it; tests/unit/picker.svelte.test.ts drives it.
import type { FluidData } from "./fluiddata.svelte";
import { find, search, type PackEntry } from "./packs";

/** The picker's value: the page's fluid, which it reads and sets. */
export interface Value {
  get(): string;
  set(name: string): void;
}

/** A fluid the registry serves, as an entry, until the index has come. */
const bare = (name: string): PackEntry => ({ name, aliases: [], cas: null, requires: [], pack: "", bytes: 0 });

export class Picker {
  /** What is typed while the list is open. */
  query = $state("");
  open = $state(false);
  /** The highlighted hit. */
  active = $state(0);

  readonly #data: FluidData;
  readonly #value: Value;

  /** The fluids to pick from. */
  readonly entries: readonly PackEntry[] = $derived.by(() =>
    this.#data.fluids.length > 0 ? this.#data.fluids : this.#data.names.map(bare),
  );
  readonly hits = $derived(search(this.entries, this.query));
  /** The input's text: the query while open, else the chosen fluid's canonical name, or the text that names no fluid. */
  readonly text = $derived.by(() => {
    if (this.open) return this.query;
    const value = this.#value.get();
    return find(this.entries, value)?.name ?? value;
  });

  constructor(data: FluidData, value: Value) {
    this.#data = data;
    this.#value = value;
  }

  /** Opens the list on every fluid, the chosen one highlighted. */
  show(): void {
    [this.query, this.open] = ["", true];
    const name = find(this.entries, this.#value.get())?.name;
    this.active = Math.max(
      0,
      this.hits.findIndex((h) => h.entry.name === name),
    );
  }

  /** New text: the list narrows to it, its best hit highlighted. */
  type(text: string): void {
    [this.query, this.open, this.active] = [text, true, 0];
  }

  /** Picks `entry`, or the highlighted hit, and closes the list. */
  pick(entry: PackEntry | undefined = this.hits[this.active]?.entry): void {
    if (entry) this.#value.set(entry.name);
    this.open = false;
  }

  dismiss(): void {
    this.open = false;
  }

  /** A key on the input: the arrows move the highlight (opening the list), Enter picks, Escape closes; whether the key
   *  was the picker's, so the page does nothing else with it. */
  key(key: string): boolean {
    if (key === "ArrowDown" || key === "ArrowUp") {
      if (!this.open) this.show();
      else this.active = Math.max(0, Math.min(this.active + (key === "ArrowDown" ? 1 : -1), this.hits.length - 1));
      return true;
    }
    if (!this.open || (key !== "Enter" && key !== "Escape")) return false;
    if (key === "Enter") this.pick();
    else this.dismiss();
    return true;
  }
}

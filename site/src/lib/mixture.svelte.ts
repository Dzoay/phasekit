// The mixture editor's view (PLAN.md M21.5): a mixture's components, fluids of the index, each with its mole fraction as
// typed, and the fluids each may become or be joined by, those the library pairs with every other component. It writes
// the page's fluid as a mixture name (src/lib/mixtures.ts), each fraction as typed, and the package reads it and checks
// that the fractions sum to one. A pure fluid is joined by another at 0.5 each; a predefined mixture is written out,
// with the package's components and fractions, to be changed. MixtureEditor.svelte binds to it;
// tests/unit/mixture.svelte.test.ts drives it.
import { invalid } from "./calculator.svelte";
import type { FluidData } from "./fluiddata.svelte";
import { nameOf, partners, partsOf, type Part } from "./mixtures";
import { find, type PackEntry } from "./packs";
import type { Value } from "./picker.svelte";

export class MixtureEditor {
  readonly #data: FluidData;
  readonly #value: Value;
  /** A fraction typed that is not a number: its row, its text, and the fluid it was typed for. */
  #draft = $state<{ fluid: string; row: number; text: string } | null>(null);

  /** The fluid's parts, where it is a mixture name `A[x]&B[y]`; null otherwise. */
  readonly parts: Part[] | null = $derived.by(() => partsOf(this.#value.get()));

  constructor(data: FluidData, value: Value) {
    this.#data = data;
    this.#value = value;
  }

  /** Row `row`'s fraction as shown: what was typed there while the fluid has stayed the same, else the name's. */
  fraction(row: number): string {
    const draft = this.#draft;
    if (draft?.row === row && draft.fluid === this.#value.get()) return draft.text;
    return this.parts?.[row]?.fraction ?? "";
  }

  /** New text for row `row`'s fraction: a number is written into the name as typed; other text is shown, and the
   *  fluid stays as it was. */
  type(row: number, text: string): void {
    if (invalid(text)) {
      this.#draft = { fluid: this.#value.get(), row, text };
      return;
    }
    this.#change((parts) => parts.map((p, i) => (i === row ? { ...p, fraction: text } : p)));
  }

  /** Row `row` becomes fluid `name`, its fraction kept. These three change a mixture name's parts; a fluid with none
   *  stays as it is. */
  rename(row: number, name: string): void {
    this.#change((parts) => parts.map((p, i) => (i === row ? { ...p, name } : p)));
  }

  /** Row `row` goes; one component left is that fluid alone. */
  remove(row: number): void {
    this.#change((parts) => parts.filter((_, i) => i !== row));
  }

  /** Fluid `name` joins: a mixture at a fraction of 0, so that the sum holds until another changes; a pure fluid at
   *  0.5 each. */
  add(name: string): void {
    const fluid = this.#value.get();
    const pure = find(this.#data.fluids, fluid)?.name ?? fluid;
    this.#write(
      this.parts
        ? [...this.parts, { name, fraction: "0" }]
        : [
            { name: pure, fraction: "0.5" },
            { name, fraction: "0.5" },
          ],
    );
  }

  /** The fluids row `row` may become, or without a row those that may join: each paired with every other component
   *  (a pure fluid's one). None where another component is not in the index. */
  options(row?: number): PackEntry[] {
    const names = this.parts?.map((p) => p.name) ?? [this.#value.get()];
    const others = names.filter((_, i) => i !== row).map((n) => find(this.#data.fluids, n));
    if (!others.every((e): e is PackEntry => e !== undefined)) return [];
    return partners(this.#data.fluids, others, this.#data.pairs);
  }

  /** The fluid written out as parts: `components` at mole fractions `feed`, the package's (a predefined mixture's), each
   *  fraction as its shortest round-trip decimal. */
  edit(components: readonly string[], feed: readonly number[]): void {
    this.#write(components.map((name, i) => ({ name, fraction: String(feed[i]) })));
  }

  /** The parts `change` makes of the fluid's, written; nothing where the fluid has none. */
  #change(change: (parts: Part[]) => Part[]): void {
    if (this.parts) this.#write(change(this.parts));
  }

  #write(parts: Part[]): void {
    this.#draft = null;
    this.#value.set(nameOf(parts));
  }
}

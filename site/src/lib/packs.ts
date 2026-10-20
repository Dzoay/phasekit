// The fluids beyond the five the browser package embeds (PLAN.md M21.4): `cargo xtask packs` writes one pack per fluid
// and packs/index.json beside the page. A fluid's pack is fetched on its first use, after the packs of the fluids it
// requires (its ECS transport references), and only where the registry does not serve the fluid already.

/** One fluid of packs/index.json. */
export interface PackEntry {
  /** Its canonical name. */
  name: string;
  /** CoolProp's other names for it, its InChIKey among them. */
  aliases: string[];
  /** Its CAS registry number, where CoolProp lists one. */
  cas: string | null;
  /** The canonical names of the fluids it requires. */
  requires: string[];
  /** Its pack's file, beside index.json. */
  pack: string;
  /** The pack's size in bytes. */
  bytes: number;
  /** Set on a predefined mixture's entry, which the picker lists after the index's fluids, its components as the
   *  fluids it requires (PLAN.md M21.5); the index has none. */
  mixture?: true;
}

export interface PackIndex {
  /** The dataset the blobs were generated from. */
  dataset: string;
  fluids: PackEntry[];
}

/** Every name an entry answers to: its canonical name, its CAS number, then its aliases. */
const namesOf = (f: PackEntry): string[] => [f.name, ...(f.cas === null ? [] : [f.cas]), ...f.aliases];

/** A name as search compares it: lower case, its letters and digits only, so "R1234ze(E)" is "r1234zee". */
export const fold = (s: string): string => s.toLowerCase().replace(/[^a-z0-9]/g, "");

/** The entry a fluid string names, as the registry resolves one: its canonical name, its CAS number or an alias, in
 *  any letter case. */
export function find(fluids: readonly PackEntry[], name: string): PackEntry | undefined {
  const key = name.toLowerCase();
  return fluids.find((f) => namesOf(f).some((n) => n.toLowerCase() === key));
}

/** A search's hit: the entry, and the CAS number or alias that matched where its name did not. */
export interface Hit {
  entry: PackEntry;
  via: string | null;
}

/** The entries one of whose names contains `query`, letter case and punctuation aside: those a name of which starts
 *  with it first, then the rest; among each, a match on the canonical name first; otherwise in the index's order. An
 *  empty query finds every entry. */
export function search(fluids: readonly PackEntry[], query: string): Hit[] {
  const q = fold(query);
  const ranked: { rank: number; hit: Hit }[] = [];
  for (const entry of fluids) {
    const ranks = namesOf(entry).map((n, i) => {
      const at = fold(n).indexOf(q);
      return at < 0 ? Infinity : (at === 0 ? 0 : 2) + (i === 0 ? 0 : 1);
    });
    const rank = Math.min(...ranks);
    const i = ranks.indexOf(rank);
    if (rank < Infinity) ranked.push({ rank, hit: { entry, via: i === 0 ? null : (namesOf(entry)[i] as string) } });
  }
  return ranked.sort((a, b) => a.rank - b.rank).map((r) => r.hit);
}

/** The packs to add, in order, for the registry to serve `name`: each fluid it requires first, with its own
 *  requirements before it, then its own; none for a fluid in `named` (the registry serves it) or one the index lacks. */
export function plan(fluids: readonly PackEntry[], name: string, named: ReadonlySet<string>): PackEntry[] {
  const order: PackEntry[] = [];
  const seen = new Set<PackEntry>();
  const visit = (n: string): void => {
    const entry = find(fluids, n);
    if (!entry || named.has(entry.name) || seen.has(entry)) return;
    seen.add(entry);
    entry.requires.forEach(visit);
    order.push(entry);
  };
  visit(name);
  return order;
}

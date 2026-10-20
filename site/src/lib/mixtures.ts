// What the page knows of mixing (PLAN.md M21.5): the library's binary pairs, by CAS number, decide which of the index's
// fluids mix with which, and so which predefined mixtures the package can build (it refuses the rest, ROT-200); and a
// mixture name `A[x]&B[y]` is read into its parts and written from them, each fraction as typed, so the page never
// computes one: the package reads the text and checks that the fractions sum to one.
import { find, type PackEntry } from "./packs";

/** One component of a mixture name: its fluid's name, and its mole fraction as the name writes it. */
export interface Part {
  name: string;
  fraction: string;
}

/** A predefined mixture the package can build: its name (`R410A.mix`) and its components' entries. */
export interface Predefined {
  name: string;
  components: PackEntry[];
}

/** A binary pair's key, as the package lists them: the two CAS numbers sorted (ASCII, so code units sort as bytes) and
 *  joined by `&`. */
// Stryker disable next-line EqualityOperator: two equal CAS numbers make one key either way.
export const pairKey = (a: string, b: string): string => (a < b ? `${a}&${b}` : `${b}&${a}`);

/** Whether the library pairs `a` with `b`: both have a CAS number, and their pair is listed. */
export function pairs(a: PackEntry, b: PackEntry, listed: ReadonlySet<string>): boolean {
  return a.cas !== null && b.cas !== null && listed.has(pairKey(a.cas, b.cas));
}

/** The fluids of `fluids` that mix with every one of `chosen`, none of them among it. */
export function partners(
  fluids: readonly PackEntry[],
  chosen: readonly PackEntry[],
  listed: ReadonlySet<string>,
): PackEntry[] {
  return fluids.filter((f) => !chosen.includes(f) && chosen.every((c) => pairs(f, c, listed)));
}

/** The predefined mixtures whose components the index has, every two of them paired. */
export function buildable(
  predefined: readonly { name: string; components: readonly string[] }[],
  fluids: readonly PackEntry[],
  listed: ReadonlySet<string>,
): Predefined[] {
  return predefined.flatMap(({ name, components }) => {
    const entries = components.map((c) => find(fluids, c));
    if (!entries.every((e): e is PackEntry => e !== undefined)) return [];
    const paired = entries.every((a, i) => entries.slice(i + 1).every((b) => pairs(a, b, listed)));
    return paired ? [{ name, components: entries }] : [];
  });
}

/** `A[x]&B[y]...`'s parts, an `HEOS::` prefix dropped, each fraction as written; null for any other name (a pure
 *  fluid's, a predefined mixture's, one the package would refuse as malformed). */
export function partsOf(fluid: string): Part[] | null {
  const body = fluid.startsWith("HEOS::") ? fluid.slice("HEOS::".length) : fluid;
  const found = body.split("&").map((part) => /^([^[\]&]+)\[([^[\]&]*)\]$/.exec(part));
  if (!found.every((m): m is RegExpExecArray => m !== null)) return null;
  return found.map((m) => ({ name: m[1] as string, fraction: m[2] as string }));
}

/** The mixture name of `parts`, each fraction as written: `A[x]&B[y]`; one part alone is its fluid's name. */
export function nameOf(parts: readonly Part[]): string {
  return parts.length === 1 ? (parts[0] as Part).name : parts.map((p) => `${p.name}[${p.fraction}]`).join("&");
}

/** Whether a mixture's refusal is of two liquids or three phases: phasekit's mixtures take every input pair, and refuse
 *  a liquid-liquid split, or a three-phase state that shows as one, as an unsupported pair (user decision MX4). */
export function liquidLiquid(refusal: string): boolean {
  return /^input pair \w+ is not supported by this model$/.test(refusal);
}

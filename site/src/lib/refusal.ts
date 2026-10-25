// A refusal in the viewer's units (PLAN.md M28.3; issue #191). The package writes its message in SI and explains a
// refusal in any units it is given; the worker asks it once per unit system the page offers, in both styles, so that
// the page switches systems without asking again. The wording is always the package's.
import type { Explained, Refusal } from "../engine/protocol";
import { KINDS, SYSTEMS, displayUnit, type System, type Unit } from "./units";

/** What the package hangs on a refusal's `Error` (its `Refusal` handle). */
interface PkRefusal {
  explain(units: Readonly<Record<string, Unit>>, style: "long" | "short", subject?: string): string;
  free(): void;
}

/** The units the package's `explain` takes for `system`: each kind's shown unit, keyed by the kind. */
export function unitsOf(system: System): Record<string, Unit> {
  return Object.fromEntries(KINDS.map((k) => [k.id, displayUnit(k.id, system)]));
}

/** What hangs on `e` as its `refusal`: the package's handle (in the worker), or an `EngineError`'s kept refusal (on the
 *  page). */
function attached(e: unknown): { handle?: PkRefusal; kept?: Refusal } {
  const r: unknown = typeof e === "object" && e !== null && "refusal" in e ? e.refusal : undefined;
  if (typeof r !== "object" || r === null) return {};
  return "explain" in r ? { handle: r as PkRefusal } : { kept: r as Refusal };
}

/** The remedy codes the package hangs on a refusal's `Error` (PLAN.md M29.6), if any. */
export function remediesOf(e: unknown): string[] {
  const r: unknown = typeof e === "object" && e !== null && "remedies" in e ? e.remedies : undefined;
  return Array.isArray(r) ? r.filter((c): c is string => typeof c === "string") : [];
}

/** A thrown refusal as the page keeps it: its message, the package's explanation in each system where it gave one,
 *  with a domain limit written as `subject`'s where one is given (PLAN.md M29.5), and its remedies. The package's
 *  handle is freed once it is read; an `EngineError` gives back the refusal it carries. */
export function refusalOf(e: unknown, subject?: string): Refusal {
  const message = e instanceof Error ? e.message : String(e);
  const { handle, kept } = attached(e);
  if (kept) return kept;
  const found = remediesOf(e);
  const remedies = found.length > 0 ? found : undefined;
  if (!handle) return { message, explained: {}, remedies };
  try {
    const explained: Partial<Record<System, Explained>> = {};
    for (const { id } of SYSTEMS) {
      const units = unitsOf(id);
      explained[id] = { long: handle.explain(units, "long", subject), short: handle.explain(units, "short", subject) };
    }
    return { message, explained, remedies };
  } finally {
    handle.free();
  }
}

/** A remedy in the site's terms: what the viewer can change on this page; null for one the site offers no way to
 *  follow (a warm start, a choice of root). */
export function remedyText(code: string): string | null {
  switch (code) {
    case "saturated_phases":
      return "Inside the two-phase region this has no single value: set the quality to 0 or 1 to read the saturated liquid's or vapour's.";
    case "extrapolate":
      return "The state lies outside the range the equation of state is valid for; the calculator does not extrapolate.";
    case "other_pair":
    case "declared_pair":
      return "Give the state by another pair of inputs.";
    default:
      return null;
  }
}

/** The refusal in `system` and `style`, or its message where the package did not explain it. */
export function say(refusal: Refusal, system: System, style: keyof Explained): string {
  return refusal.explained[system]?.[style] ?? refusal.message;
}

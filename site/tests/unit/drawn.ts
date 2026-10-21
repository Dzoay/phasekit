// The geometry the diagrams page draws, as it gets it (PLAN.md M21.6): the engine's `diagram` and `regions` answers,
// read as the view reads them, under Node on the real package; and the package's own registry beside it, to check it by.
import { expect } from "vitest";
import { readDiagram, readRegions, type DiagramData, type Regions, type Vertex } from "../../src/diagram/data";
import { createHandler } from "../../src/engine/handler";
import type { DiagramValue, RegionsValue, Request, Response } from "../../src/engine/protocol";
import { LIMIT_KEYS, limits, type Limits } from "../../src/lib/ranges";
import { loadPk, packBytes } from "../pk";

/** The engine on `fluid`, with `packs` added first, and a registry of the package's own with the same packs. */
export async function drawn(fluid: string, packs: string[] = []) {
  const pk = await loadPk("simd");
  const handle = createHandler(pk, "simd128");
  let id = 0;
  const call = (body: Record<string, unknown>): Response => handle({ fluid, ...body, id: ++id } as Request);
  let registry = new pk.Registry();
  for (const file of packs) {
    expect(call({ op: "pack", bytes: packBytes(file) }).ok, file).toBe(true);
    const next = registry.withPack(packBytes(file));
    registry.free();
    registry = next;
  }
  const answer = <T>(r: Response): T => {
    if (!r.ok) throw new Error(r.error);
    return r.value as T;
  };
  /** The diagram on `plane`, as the page draws it. */
  const diagram = (plane: string): DiagramData => readDiagram(answer<DiagramValue>(call({ op: "diagram", plane })));
  /** The phase regions, as the page draws them. */
  const regions = (): Regions => readRegions(answer<RegionsValue>(call({ op: "regions" })));
  const constants = answer<(number | null)[]>(call({ op: "constants", keys: [...LIMIT_KEYS] }));
  /** The package's values of `outputs` at a state; an empty list where it refuses the state. */
  const at = (outputs: string[], name1: string, value1: number, name2: string, value2: number): number[] => {
    const r = call({ op: "state", name1, value1, name2, value2, outputs });
    return r.ok && r.op === "state" ? (r.value.values as number[]) : [];
  };
  /** The package's phase at a state, null where it refuses it. */
  const phaseAt = (name1: string, value1: number, name2: string, value2: number): string | null => {
    const r = call({ op: "state", name1, value1, name2, value2, outputs: [] });
    return r.ok && r.op === "state" ? r.value.phase : null;
  };
  /** PropsSI at a vertex's inputs, null where the package refuses it. */
  const props = (key: string, v: Vertex): number | null => {
    try {
      return registry.PropsSI(key, ...v.inputs, fluid);
    } catch {
      return null;
    }
  };
  return { pk, registry, diagram, regions, limits: limits(constants) as Limits, constants, at, phaseAt, props };
}

/** Every vertex of a diagram: the dome's, the critical point's and each line's. */
export function vertices(data: DiagramData): Vertex[] {
  const dome = [...data.dome.liquid, ...data.dome.vapour, ...(data.dome.critical ? [data.dome.critical] : [])];
  return [...dome, ...data.lines.flatMap((l) => l.segments.flatMap((s) => s.vertices))];
}

/** A vertex's inputs by key. */
export function inputsOf(v: Vertex): Record<string, number> {
  return { [v.inputs[0]]: v.inputs[1], [v.inputs[2]]: v.inputs[3] };
}

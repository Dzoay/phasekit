// A mixture's diagram (PLAN.md M21.5) under Node on the real package, R410A.mix from R32's and R125's packs: every
// vertex of its envelope on p–T and T–ρ is the package's envelope point (`Exact`), drawn bubble side, critical point,
// dew side; every vertex more than 1 mK from the critical temperature is the package's QT or PQ state at its own Q
// within `Flash` (VERIFICATION.md §5; #257), nearer which each side turns in T and in p, so a given T or p has two
// points within 1 Pa; the critical point drawn is the model's, which lies on the envelope within TM15's stated 0.02 K
// and 2 kPa.
import { describe, expect, test } from "vitest";
import { createHandler } from "../../src/engine/handler";
import type { EnvelopeValue, Request } from "../../src/engine/protocol";
import { diagram, type Diagram } from "../../src/sample/diagrams";
import { MIXTURE_DIAGRAMS, MIXTURE_KEYS, around, sampleEnvelope } from "../../src/sample/envelope";
import type { Vertex } from "../../src/sample/lines";
import { loadPk, packBytes, sameBits } from "../pk";

/** R410A.mix on the engine and on a registry of the package's own, both with R32's and R125's packs. */
async function r410a() {
  const pk = await loadPk("simd");
  const handle = createHandler(pk, "simd128");
  let registry = new pk.Registry();
  for (const file of ["r32.pack", "r125.pack"]) {
    expect(handle({ id: 0, op: "pack", bytes: packBytes(file) }).ok).toBe(true);
    const next = registry.withPack(packBytes(file));
    registry.free();
    registry = next;
  }
  const ask = <T>(request: Request): T => {
    const r = handle(request);
    if (!r.ok) throw new Error(r.error);
    return r.value as T;
  };
  const envelope = ask<EnvelopeValue>({ id: 1, op: "envelope", fluid: "R410A.mix" });
  const constants = ask<(number | null)[]>({ id: 2, op: "constants", fluid: "R410A.mix", keys: [...MIXTURE_KEYS] });
  return { registry, envelope, constants };
}

const plane = (id: string) => diagram(id) as Diagram;

describe("a mixture's envelope on the real package", () => {
  test("every vertex is the package's envelope point, drawn from the bubble side through the critical point", async () => {
    const { envelope: e, constants } = await r410a();
    for (const id of MIXTURE_DIAGRAMS) {
      const d = plane(id);
      const data = sampleEnvelope(d, e, constants);
      const at = (k: number): Record<string, number> => ({
        T: e.T[k] as number,
        P: e.P[k] as number,
        Dmass: e.Dmass[k] as number,
      });
      const { liquid, vapour, critical } = data.dome;
      // The trace starts at the bubble point at 1 bar, so the bubble side is in its order, the dew side reversed.
      const order = [...liquid, ...[...vapour].reverse()].map((v) => v.inputs);
      expect(order).toEqual(Array.from(e.T, (t, k) => ["Q", e.Q[k], "T", t]));
      expect(liquid.every((v) => v.inputs[1] === 0) && vapour.every((v) => v.inputs[1] === 1)).toBe(true);
      for (const [k, v] of [...liquid, ...[...vapour].reverse()].entries()) {
        expect(sameBits(v.x, at(k)[d.x.key] as number) && sameBits(v.y, at(k)[d.y.key] as number), `${id} ${k}`).toBe(
          true,
        );
      }
      const [tc, pc, rhoc] = constants as number[];
      const model: Record<string, number> = { T: tc as number, P: pc as number, Dmass: rhoc as number };
      expect([critical?.x, critical?.y]).toEqual([model[d.x.key], model[d.y.key]]);
      expect(data.lines).toEqual([]);
    }
  });

  test("every vertex more than 1 mK from the critical temperature is the package's QT or PQ state within Flash", async () => {
    const { registry, envelope: e, constants } = await r410a();
    const tc = constants[0] as number;
    const answer = (output: string, ...inputs: [string, number, string, number]): number | null => {
      try {
        return registry.PropsSI(output, ...inputs, "R410A.mix");
      } catch {
        return null;
      }
    };
    const vertices: Vertex[] = [...sampleEnvelope(plane("pt"), e, [null, null, null]).dome.liquid];
    vertices.push(...sampleEnvelope(plane("pt"), e, [null, null, null]).dome.vapour);
    const away = vertices.filter((v) => Math.abs(v.x - tc) > 1e-3);
    const misses: string[] = [];
    for (const v of away) {
      const [q, t, p] = [v.inputs[1], v.x, v.y];
      const [atT, atP] = [answer("P", "Q", q, "T", t), answer("T", "P", p, "Q", q)];
      const agrees = atT !== null && Math.abs(atT / p - 1) <= 1e-9 && atP !== null && Math.abs(atP / t - 1) <= 1e-9;
      if (!agrees) misses.push(`Q ${q} at ${t} K, ${p} Pa: QT ${atT}, PQ ${atP}`);
    }
    expect(misses).toEqual([]);
    expect(away.length).toBeGreaterThanOrEqual(vertices.length - 2);
  });

  test("the critical point drawn, the model's, lies on the envelope within TM15's 0.02 K and 2 kPa", async () => {
    const { envelope: e, constants } = await r410a();
    const crossing = e.critical as [number, number];
    const critical = sampleEnvelope(plane("pt"), e, constants).dome.critical as Vertex;
    expect(Math.abs(critical.x - crossing[0])).toBeLessThanOrEqual(0.02);
    expect(Math.abs(critical.y - crossing[1])).toBeLessThanOrEqual(2000);
    expect(critical.inputs).toEqual(["T", constants[0], "Dmass", constants[2]]);
  });
});

/** An envelope of points (Q, T, p, ρ), in the trace's order, that passed a critical point or none. */
function traced(points: [number, number, number, number][], critical: [number, number] | null = [300, 5e6]) {
  const column = (i: number) => Float64Array.from(points, (p) => p[i] as number);
  return {
    Q: column(0),
    T: column(1),
    P: column(2),
    Dmass: column(3),
    incipient: new Float64Array(points.length * 2),
    components: ["A", "B"],
    feed: [0.25, 0.75],
    critical,
    end: critical ? "closed" : "bound",
    reason: critical ? "" : "outside the validity domain",
  } satisfies EnvelopeValue;
}

describe("a mixture's envelope, drawn", () => {
  const CRITICAL = [301, 5.1e6, 400];

  test("from the dew side where the trace starts there, each side toward the critical point", () => {
    const e = traced([
      [1, 200, 1e5, 2],
      [1, 280, 3e6, 100],
      [0, 290, 4e6, 600],
      [0, 210, 1e5, 1200],
    ]);
    const { dome } = sampleEnvelope(plane("pt"), e, CRITICAL);
    expect(dome.liquid.map((v) => [v.x, v.y])).toEqual([
      [210, 1e5],
      [290, 4e6],
    ]);
    expect(dome.vapour.map((v) => [v.x, v.y])).toEqual([
      [200, 1e5],
      [280, 3e6],
    ]);
    expect(dome.critical).toEqual({ x: 301, y: 5.1e6, inputs: ["T", 301, "Dmass", 400] });
    expect(dome.liquid[0]?.inputs).toEqual(["Q", 0, "T", 210]);
  });

  test("on T–ρ, the feed's density against its temperature", () => {
    const e = traced([
      [0, 210, 1e5, 1200],
      [1, 200, 1e5, 2],
    ]);
    const data = sampleEnvelope(plane("trho"), e, CRITICAL);
    expect([data.dome.liquid[0]?.x, data.dome.liquid[0]?.y, data.dome.vapour[0]?.x]).toEqual([1200, 210, 2]);
    expect(data.dome.critical).toMatchObject({ x: 400, y: 301 });
    expect([data.x, data.y]).toEqual([
      [1, 2400],
      [189.9, 311.1],
    ]);
  });

  test("without a critical point where the trace passed none, or the model gives none", () => {
    const open = traced([[0, 216, 7e6, 1100]], null);
    const data = sampleEnvelope(plane("pt"), open, CRITICAL);
    expect([data.dome.liquid.length, data.dome.vapour.length, data.dome.critical]).toEqual([1, 0, null]);
    expect(data.envelope).toEqual({
      components: ["A", "B"],
      feed: [0.25, 0.75],
      end: "bound",
      reason: "outside the validity domain",
    });
    expect([data.x, data.y]).toEqual([
      [194.4, 237.6],
      [3.5e6, 1.4e7],
    ]);
    const closed = traced([[0, 216, 7e6, 1100]]);
    for (const constants of [[null, 5e6, 400], [301, Number.NaN, 400], [301, 5e6, null], []]) {
      expect(sampleEnvelope(plane("pt"), closed, constants).dome.critical, JSON.stringify(constants)).toBeNull();
    }
  });

  test("refuses an envelope without a point, in the package's words", () => {
    const empty = { ...traced([], null), reason: "Vle did not converge in 0 iterations" };
    expect(() => sampleEnvelope(plane("pt"), empty, CRITICAL)).toThrow(
      "the package traced no point of the envelope: Vle did not converge in 0 iterations",
    );
  });

  test("its ranges widen a logarithmic axis by 2 each way, a linear one by a tenth of its span or value", () => {
    expect(around(1e5, 1e6, true)).toEqual([5e4, 2e6]);
    expect(around(200, 300, false)).toEqual([190, 310]);
    expect(around(200, 200, false)).toEqual([180, 220]);
    expect(around(-200, -200, false)).toEqual([-220, -180]);
  });
});

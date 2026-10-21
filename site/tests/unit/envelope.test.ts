// A mixture's diagram as the page draws it (PLAN.md M21.5, M21.6) under Node on the real package, R410A.mix from R32's
// and R125's packs: every vertex of its envelope on p–T and T–ρ is the package's envelope point (`Exact`), drawn bubble
// side, critical point, dew side; every vertex more than 1 mK from the critical temperature is the package's QT or PQ
// state at its own Q within `Flash` (VERIFICATION.md §5; #257), nearer which each side turns in T and in p, so a given
// T or p has two points within 1 Pa; the critical point drawn is the model's, which lies on the envelope within TM15's
// stated 0.02 K and 2 kPa.
import { describe, expect, test } from "vitest";
import type { Vertex } from "../../src/diagram/data";
import { MIXTURE_DIAGRAMS, diagram, type Diagram } from "../../src/diagram/planes";
import { sameBits } from "../pk";
import { drawn, inputsOf } from "./drawn";

const MIXTURE_KEYS = ["T_critical", "p_critical", "rhomass_critical"];

/** R410A.mix as the page draws it, and its envelope and critical constants as the package gives them. */
async function r410a() {
  const e = await drawn("R410A.mix", ["r32.pack", "r125.pack"]);
  const fluid = e.registry.fluid("R410A.mix");
  const traced = fluid.envelope();
  const envelope = {
    T: traced.T,
    P: traced.P,
    Q: traced.Q,
    Dmass: traced.Dmass,
    critical: traced.criticalT === undefined ? null : [traced.criticalT, traced.criticalP as number],
  };
  traced.free();
  fluid.free();
  const constants = MIXTURE_KEYS.map((key) => e.registry.Props1SI(key, "R410A.mix"));
  return { ...e, envelope, constants };
}

const plane = (id: string) => diagram(id) as Diagram;

describe("a mixture's envelope, drawn", () => {
  test("every vertex is the package's envelope point, drawn from the bubble side through the critical point", async () => {
    const { diagram: draw, envelope: e, constants } = await r410a();
    for (const id of MIXTURE_DIAGRAMS) {
      const d = plane(id);
      const data = draw(id);
      const at = (k: number): Record<string, number> => ({
        T: e.T[k] as number,
        P: e.P[k] as number,
        Dmass: e.Dmass[k] as number,
      });
      const { liquid, vapour, critical } = data.dome;
      // The trace starts at the bubble point at 1 bar, so the bubble side is in its order, the dew side reversed.
      const order = [...liquid, ...[...vapour].reverse()].map(inputsOf);
      expect(order).toEqual(Array.from(e.T, (t, k) => ({ Q: e.Q[k], T: t })));
      expect(liquid.every((v) => inputsOf(v).Q === 0) && vapour.every((v) => inputsOf(v).Q === 1)).toBe(true);
      for (const [k, v] of [...liquid, ...[...vapour].reverse()].entries()) {
        expect(sameBits(v.x, at(k)[d.x.key] as number) && sameBits(v.y, at(k)[d.y.key] as number), `${id} ${k}`).toBe(
          true,
        );
      }
      const [tc, pc, rhoc] = constants as number[];
      const model: Record<string, number> = { T: tc as number, P: pc as number, Dmass: rhoc as number };
      expect([critical?.x, critical?.y]).toEqual([model[d.x.key], model[d.y.key]]);
      expect(data.lines).toEqual([]);
      expect(data.envelope).toEqual({
        components: ["R32", "R125"],
        feed: [expect.closeTo(0.6976, 4), expect.closeTo(0.3024, 4)],
        end: "closed",
        reason: "",
      });
    }
  });

  test("every vertex more than 1 mK from the critical temperature is the package's QT or PQ state within Flash", async () => {
    const { diagram: draw, registry, constants } = await r410a();
    const tc = constants[0] as number;
    const answer = (output: string, ...inputs: [string, number, string, number]): number | null => {
      try {
        return registry.PropsSI(output, ...inputs, "R410A.mix");
      } catch {
        return null;
      }
    };
    const { liquid, vapour } = draw("pt").dome;
    const all: Vertex[] = [...liquid, ...vapour];
    const away = all.filter((v) => Math.abs(v.x - tc) > 1e-3);
    const misses: string[] = [];
    for (const v of away) {
      const [q, t, p] = [inputsOf(v).Q as number, v.x, v.y];
      const [atT, atP] = [answer("P", "Q", q, "T", t), answer("T", "P", p, "Q", q)];
      const agrees = atT !== null && Math.abs(atT / p - 1) <= 1e-9 && atP !== null && Math.abs(atP / t - 1) <= 1e-9;
      if (!agrees) misses.push(`Q ${q} at ${t} K, ${p} Pa: QT ${atT}, PQ ${atP}`);
    }
    expect(misses).toEqual([]);
    expect(away.length).toBeGreaterThanOrEqual(all.length - 2);
  });

  test("the critical point drawn, the model's, lies on the envelope within TM15's 0.02 K and 2 kPa", async () => {
    const { diagram: draw, envelope: e, constants } = await r410a();
    const crossing = e.critical as [number, number];
    const critical = draw("pt").dome.critical as Vertex;
    expect(Math.abs(critical.x - crossing[0])).toBeLessThanOrEqual(0.02);
    expect(Math.abs(critical.y - crossing[1])).toBeLessThanOrEqual(2000);
    expect(inputsOf(critical)).toEqual({ T: constants[0], Dmass: constants[2] });
  });

  test("on the other planes is the package's refusal", async () => {
    const { diagram: draw } = await r410a();
    const pairs: [string, string][] = [
      ["ph", "HP"],
      ["ts", "ST"],
      ["hs", "HS"],
    ];
    for (const [id, pair] of pairs)
      expect(() => draw(id), id).toThrow(`input pair ${pair} is not supported by this model`);
  });
});

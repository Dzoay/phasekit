// The surface's placement in the cube (PLAN.md M26.2): src/surface/mesh.ts, on a mesh made here and on Water's, the
// package's.
import { describe, expect, test } from "vitest";
import type { SurfaceValue } from "../../src/engine/protocol";
import {
  KINDS,
  domePath,
  kindOf,
  phasesPresent,
  place,
  positions,
  span,
  surfaceToken,
  valuesOf,
  vertexColours,
} from "../../src/surface/mesh";
import { loadPk } from "../pk";

/** Three vertices: a liquid, a two-phase state and a gas, one triangle. */
function small(): SurfaceValue {
  return {
    T: new Float64Array([300, 400, 500]),
    rho: new Float64Array([1000, 10, 0.1]),
    p: new Float64Array([1e5, 1e6, 1e4]),
    u: new Float64Array([1, 2, 3]),
    s: new Float64Array([-1, 0, 1]),
    Q: new Float64Array([NaN, 0.5, NaN]),
    phases: new Uint8Array([0, 1, 2]),
    phaseNames: ["liquid", "twophase", "gas"],
    triangles: new Uint32Array([0, 1, 2]),
    liquid: new Uint32Array([0]),
    vapour: new Uint32Array([2]),
    critical: new Uint32Array([1]),
    triple: new Uint32Array([]),
  };
}

describe("the surface's placement", () => {
  test("names its kinds and their axes, p–v–T for anything a URL names but usv", () => {
    expect(KINDS.pvt).toEqual({ name: "p–v–T", axes: ["v", "p", "T"] });
    expect(KINDS.usv).toEqual({ name: "u–s–v", axes: ["s", "u", "v"] });
    expect([kindOf("usv"), kindOf("pvt"), kindOf(null), kindOf("USV")]).toEqual(["usv", "pvt", "pvt", "pvt"]);
  });

  test("reads v as 1/ρ and the others as the package gives them", () => {
    const m = small();
    expect([...valuesOf(m, "v")]).toEqual([0.001, 0.1, 10]);
    expect(valuesOf(m, "p")).toBe(m.p);
    expect(valuesOf(m, "T")).toBe(m.T);
    expect(valuesOf(m, "u")).toBe(m.u);
    expect(valuesOf(m, "s")).toBe(m.s);
  });

  test("spans the finite values; a span of none is NaN", () => {
    expect(span(new Float64Array([3, NaN, -2, Infinity, 5]))).toEqual([-2, 5]);
    expect(span(new Float64Array([4]))).toEqual([4, 4]);
    expect(span(new Float64Array([NaN, -Infinity]))).toEqual([NaN, NaN]);
    expect(span(new Float64Array([]))).toEqual([NaN, NaN]);
  });

  test("places a value from −1 to 1 across its span, a logarithmic axis in its logarithm, a point's at 0", () => {
    const linear = { key: "T", log: false, span: [100, 300] } as const;
    expect([place(100, linear), place(200, linear), place(300, linear), place(150, linear)]).toEqual([-1, 0, 1, -0.5]);
    const log = { key: "p", log: true, span: [10, 1000] } as const;
    expect([place(10, log), place(100, log), place(1000, log)]).toEqual([-1, 0, 1]);
    expect(place(7, { key: "s", log: false, span: [7, 7] })).toBe(0);
    expect(place(7, { key: "s", log: false, span: [8, 7] })).toBe(0);
  });

  test("places each vertex on p–v–T's axes and u–s–v's, three a vertex", () => {
    const m = small();
    const pvt = positions(m, "pvt");
    expect(pvt.axes).toEqual([
      { key: "v", log: true, span: [0.001, 10] },
      { key: "p", log: true, span: [1e4, 1e6] },
      { key: "T", log: false, span: [300, 500] },
    ]);
    // T deep, away from the viewer: −z.
    expect([...pvt.xyz]).toEqual([-1, 0, 1, 0, 1, -0, 1, -1, -1]);
    const usv = positions(m, "usv");
    expect(usv.axes.map((a) => [a.key, a.log])).toEqual([
      ["s", false],
      ["u", false],
      ["v", true],
    ]);
    expect([...usv.xyz]).toEqual([-1, -1, 1, 0, 0, -0, 1, 1, -1]);
  });

  test("runs the dome up the liquid, through the critical point, down the vapour", () => {
    const m = { ...small(), liquid: new Uint32Array([0, 1]), vapour: new Uint32Array([4, 3]) };
    expect(domePath({ ...m, critical: new Uint32Array([2]) })).toEqual([0, 1, 2, 3, 4]);
    expect(domePath({ ...m, critical: new Uint32Array([]) })).toEqual([0, 1, 3, 4]);
  });

  test("colours each vertex by its phase, black for a code with no name", () => {
    const m = { ...small(), phases: new Uint8Array([2, 0, 7]) };
    const rgb = (phase: string) => (phase === "gas" ? ([0, 0, 1] as const) : ([1, 0.5, 0] as const));
    expect([...vertexColours(m, rgb)]).toEqual([0, 0, 1, 1, 0.5, 0, 0, 0, 0]);
  });

  test("names each phase's surface colour token", () => {
    expect(["liquid", "twophase", "supercritical_gas"].map(surfaceToken)).toEqual([
      "--surface-liquid",
      "--surface-twophase",
      "--surface-supercritical-gas",
    ]);
  });

  test("lists the phases present in the legend's order", () => {
    const m = { ...small(), phases: new Uint8Array([2, 0, 2]) };
    expect(phasesPresent(m, ["twophase", "gas", "supercritical", "liquid"])).toEqual(["gas", "liquid"]);
  });

  test(
    "places Water's surface, the package's, inside the cube, its dome on the mesh",
    { timeout: 30_000 },
    async () => {
      const pk = await loadPk("simd");
      const registry = new pk.Registry();
      const water = registry.fluid("Water");
      const handle = water.surface();
      const m: SurfaceValue = {
        T: handle.T,
        rho: handle.rho,
        p: handle.p,
        u: handle.u,
        s: handle.s,
        Q: handle.Q,
        phases: handle.phases,
        phaseNames: handle.phaseNames,
        triangles: handle.triangles,
        liquid: handle.liquid,
        vapour: handle.vapour,
        critical: handle.critical,
        triple: handle.triple,
      };
      for (const h of [handle, water, registry]) h.free();
      for (const kind of ["pvt", "usv"] as const) {
        const { xyz, axes } = positions(m, kind);
        expect(xyz.length).toBe(3 * m.T.length);
        expect(xyz.every((x) => x >= -1 && x <= 1)).toBe(true);
        expect(axes.every((a) => a.span[0] < a.span[1])).toBe(true);
      }
      const dome = domePath(m);
      expect(dome.length).toBe(m.liquid.length + m.critical.length + m.vapour.length);
      expect(dome.every((i) => i < m.T.length)).toBe(true);
      expect(phasesPresent(m, ["liquid", "twophase", "gas"])).toEqual(["liquid", "twophase", "gas"]);
    },
  );
});

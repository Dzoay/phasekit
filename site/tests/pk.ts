// The browser package as the tests load it under Node: the module the page runs, from the bytes of its .wasm (Node's
// fetch reads no file: URLs). scripts/site/build.sh puts both builds in src/pk.
import { readFileSync } from "node:fs";
import type { Pk } from "../src/engine/pk";

export type Build = "baseline" | "simd";

const loaded = new Map<Build, Promise<Pk>>();

async function load(build: Build): Promise<Pk> {
  const pk = (await import(`../src/pk/${build}/phasekit.js`)) as Pk;
  await pk.default({ module_or_path: readFileSync(new URL(`../src/pk/${build}/phasekit_bg.wasm`, import.meta.url)) });
  return pk;
}

/** One build of the package, initialised once per test process. */
export function loadPk(build: Build): Promise<Pk> {
  let pk = loaded.get(build);
  if (!pk) {
    pk = load(build);
    loaded.set(build, pk);
  }
  return pk;
}

/** `PropsSI` as the package answers it, on a registry made and freed for the call. */
export async function propsSI(
  build: Build,
  output: string,
  name1: string,
  value1: number,
  name2: string,
  value2: number,
  fluid: string,
): Promise<number> {
  const pk = await loadPk(build);
  const registry = new pk.Registry();
  try {
    return registry.PropsSI(output, name1, value1, name2, value2, fluid);
  } finally {
    registry.free();
  }
}

/** Two numbers with the same bits (NaN payloads aside, which no test compares). */
export function sameBits(a: number, b: number): boolean {
  return Object.is(a, b);
}

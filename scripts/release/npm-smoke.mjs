// npm-smoke.mjs <dir>: the packed browser modules under Node before they ship (PLAN.md §5.4 item 7): each build in
// <dir> (baseline/ and simd/) loads, answers PropsSI with Water's normal boiling point, and runs a batch over typed
// arrays; every handle object is freed.
import { readFileSync } from "node:fs";

const dir = process.argv[2];
for (const build of ["baseline", "simd"]) {
  const pkg = await import(`${dir}/${build}/phasekit.js`);
  await pkg.default({ module_or_path: readFileSync(`${dir}/${build}/phasekit_bg.wasm`) });
  const registry = new pkg.Registry();
  const t = registry.PropsSI("T", "P", 101325, "Q", 0, "Water");
  const fluid = registry.fluid("Water");
  const batch = fluid.evaluate(["Hmass"], "T", new Float64Array([300, -1]), "P", new Float64Array([1e5, 1e5]));
  const ok = t > 373.124 && t < 373.125 && batch.values instanceof Float64Array && batch.status[0] === 0 && batch.status[1] === 2;
  batch.free();
  fluid.free();
  registry.free();
  if (!ok) {
    console.error(`npm-smoke: the ${build} build failed: T = ${t}`);
    process.exit(1);
  }
  console.log(`npm-smoke: ${build} ok, T = ${t} K`);
}

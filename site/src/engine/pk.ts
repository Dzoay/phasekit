// The browser package's module, as wasm-bindgen writes it into src/pk (scripts/site/build.sh). Both builds have one
// interface; the simd128 one names the type.
export type Pk = typeof import("../pk/simd/phasekit.js");

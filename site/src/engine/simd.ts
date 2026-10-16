// Whether this browser runs WebAssembly SIMD: it validates a module whose one function uses v128 (`i32.const 0`,
// `i8x16.splat`, `i8x16.popcnt`), the probe wasm-feature-detect uses.
const PROBE = new Uint8Array([
  0, 97, 115, 109, 1, 0, 0, 0, 1, 5, 1, 96, 0, 1, 123, 3, 2, 1, 0, 10, 10, 1, 8, 0, 65, 0, 253, 15, 253, 98, 11,
]);

export function simdSupported(): boolean {
  try {
    return WebAssembly.validate(PROBE);
  } catch {
    return false;
  }
}

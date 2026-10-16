// A Node test environment whose modules Vite transforms for the client (PLAN.md M21.7): Svelte's runes then compile to
// the client runtime, where effects run, so tests/unit/*.svelte.test.ts can drive the calculator's wiring. No DOM.
import type { Environment } from "vitest/environments";

export default {
  name: "client",
  viteEnvironment: "client",
  setup: () => ({ teardown: () => {} }),
} satisfies Environment;

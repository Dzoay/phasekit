// The site (PLAN.md M21): a relative base, so the build serves from /phasekit/ on GitHub Pages and from / in preview;
// the commit it was built from, for the footer; ES-module workers, which import the package's ES modules.
import { svelte } from "@sveltejs/vite-plugin-svelte";
import { defineConfig } from "vitest/config";

export default defineConfig({
  base: "./",
  plugins: [svelte()],
  define: { __PHASEKIT_COMMIT__: JSON.stringify(process.env.PHASEKIT_COMMIT ?? "dev") },
  worker: { format: "es" },
  // Under Vitest, packages' browser entry points, as Svelte's testing guide says: its client runtime, where effects run.
  resolve: process.env.VITEST ? { conditions: ["browser"] } : undefined,
  build: { target: "es2022" },
  // The unit tests run in Node with modules transformed for the client, so rune tests' effects run (tests/env).
  test: {
    include: ["tests/unit/**/*.test.ts"],
    environment: "./tests/env/client.ts",
  },
});

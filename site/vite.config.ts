// The site (PLAN.md M21): a relative base, so the build serves from /phasekit/ on GitHub Pages and from / in preview;
// the commit it was built from, for the footer; ES-module workers, which import the package's ES modules.
import { svelte } from "@sveltejs/vite-plugin-svelte";
import { defineConfig } from "vitest/config";

export default defineConfig({
  base: "./",
  plugins: [svelte()],
  define: { __PHASEKIT_COMMIT__: JSON.stringify(process.env.PHASEKIT_COMMIT ?? "dev") },
  worker: { format: "es" },
  build: { target: "es2022" },
  test: { include: ["tests/unit/**/*.test.ts"] },
});

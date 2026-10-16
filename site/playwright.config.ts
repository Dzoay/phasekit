// The site's browser tests (PLAN.md M21.1): Chromium against `vite preview` of the built page, or against the page at
// PW_BASE_URL (the deployed site, for a smoke test), which starts no server.
import { defineConfig, devices } from "@playwright/test";

export default defineConfig({
  testDir: "tests/e2e",
  forbidOnly: !!process.env.CI,
  reporter: process.env.CI ? [["list"], ["html", { open: "never" }]] : "list",
  use: { baseURL: process.env.PW_BASE_URL ?? "http://localhost:4173/", trace: "retain-on-failure" },
  // A missing screenshot baseline is written, and CI's site job then fails until it is committed (ci.yml); locally the
  // screenshot tests are skipped (tests/e2e/diagrams.spec.ts), so nothing is written here.
  updateSnapshots: "missing",
  projects: [{ name: "chromium", use: { ...devices["Desktop Chrome"] } }],
  webServer: process.env.PW_BASE_URL
    ? undefined
    : { command: "npm run preview", url: "http://localhost:4173/", reuseExistingServer: !process.env.CI },
});

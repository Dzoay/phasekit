// The characteristic curves and the compressibility chart in Chromium (PLAN.md M26.6; user decision DG10): a fluid's
// four curves whole on log p against log T over its saturation line, its chart, reached from the header, and a
// mixture refused in the package's words.
import { expect, test, type Page } from "@playwright/test";

async function ready(page: Page, url: string): Promise<void> {
  await page.goto(url);
  await expect(page.locator(".plot")).toHaveAttribute("data-state", "ready", { timeout: 30_000 });
}

test("draws Nitrogen's four curves over its saturation line, and its compressibility chart", async ({ page }) => {
  await ready(page, "./#/curves?fluid=Nitrogen");
  // The saturation line and at least one line a curve.
  expect(Number(await page.getByTestId("curves-chart").getAttribute("data-paths"))).toBeGreaterThanOrEqual(5);
  expect(Number(await page.getByTestId("z-chart").getAttribute("data-paths"))).toBeGreaterThan(5);
  for (const name of ["Ideal", "Boyle", "JouleThomsonInversion", "JouleInversion"]) {
    await expect(page.locator(`[data-curve="${name}"]`)).toBeVisible();
  }
  // Nitrogen's ideal and Boyle curves meet zero density below its T_max (2000 K); its Joule inversion curve does not.
  await expect(page.locator('[data-curve="Boyle"]')).toContainText(/meets zero density at \d+/);
  await expect(page.locator('[data-curve="JouleInversion"]')).toContainText("does not meet zero density");
  await expect(page.getByTestId("z-chart").getByText("K", { exact: false }).first()).toBeVisible();
});

test("is reached from the header, and writes its fluid in the URL", async ({ page }) => {
  await page.goto("./#/calculator");
  await page.getByRole("navigation", { name: "Pages" }).getByRole("link", { name: "Curves" }).click();
  await expect(page.locator(".plot")).toHaveAttribute("data-state", "ready", { timeout: 30_000 });
  await expect(page).toHaveURL(/#\/curves\?fluid=Nitrogen$/);
});

test("refuses a mixture in the package's words", async ({ page }) => {
  await page.goto("./#/curves?fluid=R410A.mix");
  await expect(page.locator(".plot")).toHaveAttribute("data-state", "refused", { timeout: 30_000 });
  await expect(page.getByRole("alert")).toContainText("mixture");
});

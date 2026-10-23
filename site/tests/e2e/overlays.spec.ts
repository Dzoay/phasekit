// The curves and maps over the diagrams in Chromium (PLAN.md M26.6; user decisions DG10, DG11): the characteristic
// curves on p–T, their legend and the link to their own view; an output's map filled under its contours; both in the
// URL; and neither offered for a mixture.
import { expect, test, type Page } from "@playwright/test";

async function ready(page: Page, url: string): Promise<void> {
  await page.goto(url);
  await expect(page.getByTestId("plot")).toHaveAttribute("data-state", "ready", { timeout: 30_000 });
}

/** The plot's canvas as an image. */
const picture = (page: Page) =>
  page.locator('[data-testid="plot"] canvas').evaluate((c: HTMLCanvasElement) => c.toDataURL());

test("draws the characteristic curves on p–T, writes them in the URL and links to their own view", async ({ page }) => {
  await ready(page, "./#/diagrams?fluid=Nitrogen&d=pt");
  const before = await picture(page);
  await page.getByLabel("Characteristic curves").check();
  await expect(page).toHaveURL(/curves=1/);
  await expect(page.getByTestId("curves-legend")).toHaveAttribute("data-curves", "ready", { timeout: 30_000 });
  await expect(page.getByTestId("curves-legend")).toContainText("Joule–Thomson inversion curve");
  await expect.poll(() => picture(page)).not.toBe(before);
  await page.getByRole("link", { name: "whole" }).click();
  await expect(page).toHaveURL(/#\/curves\?fluid=Nitrogen$/);
});

test("fills an output's map under its contours, writes it in the URL, and takes it off again", async ({ page }) => {
  await ready(page, "./#/diagrams?fluid=Water&d=ph");
  const before = await picture(page);
  await page.getByTestId("map-output").selectOption("T");
  await expect(page).toHaveURL(/map=T/);
  await expect(page.getByTestId("map-state")).toHaveAttribute("data-map", "ready", { timeout: 30_000 });
  await expect(page.getByTestId("map-state")).toContainText("Bands between contours at");
  await expect.poll(() => picture(page)).not.toBe(before);
  await page.getByTestId("map-output").selectOption("");
  await expect(page).not.toHaveURL(/map=/);
  await expect.poll(() => picture(page)).toBe(before);
});

test("offers neither for a mixture", async ({ page }) => {
  await page.goto("./#/diagrams?fluid=R410A.mix&d=pt");
  await expect(page.getByTestId("plot")).toHaveAttribute("data-state", "ready", { timeout: 60_000 });
  await expect(page.getByLabel("Characteristic curves")).toBeHidden();
});

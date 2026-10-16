// The phase regions in Chromium (PLAN.md M21.9): the toggle is in the URL, the plot is tinted by region and each
// region is named; and their look, against baselines made on CI's runner.
import { expect, test, type Page } from "@playwright/test";

async function ready(page: Page, url: string): Promise<void> {
  await page.goto(url);
  await expect(page.getByTestId("plot")).toHaveAttribute("data-state", "ready", { timeout: 30_000 });
}

/** How many of the canvas's pixels are each phase's tint, exactly: the tints are painted opaque. */
async function tinted(page: Page): Promise<Record<string, number>> {
  return page.evaluate(() => {
    const canvas = document.querySelector<HTMLCanvasElement>('[data-testid="plot"] canvas');
    const context = canvas?.getContext("2d");
    if (!canvas || !context) return {};
    const probe = document.createElement("span");
    document.body.append(probe);
    const tints = new Map<string, string>();
    for (const phase of ["liquid", "twophase", "gas", "supercritical-liquid", "supercritical", "supercritical-gas"]) {
      probe.style.color = `var(--phase-${phase})`;
      const [r, g, b] = (getComputedStyle(probe).color.match(/\d+/g) ?? []).map(Number);
      tints.set(`${r},${g},${b}`, phase);
    }
    probe.remove();
    const counts: Record<string, number> = {};
    const { data } = context.getImageData(0, 0, canvas.width, canvas.height);
    for (let i = 0; i < data.length; i += 4) {
      const phase = tints.get(`${data[i]},${data[i + 1]},${data[i + 2]}`);
      if (phase && data[i + 3] === 255) counts[phase] = (counts[phase] ?? 0) + 1;
    }
    return counts;
  });
}

test("the toggle is in the URL, and tints and names each region", async ({ page }) => {
  await ready(page, "./#/diagrams?fluid=Water&d=ph");
  expect(await tinted(page)).toEqual({});
  await page.getByLabel("Tint the phase regions").check();
  await expect(page).toHaveURL(/regions=1/);
  const plot = page.getByTestId("plot");
  await expect(plot).toHaveAttribute("data-regions", "ready", { timeout: 30_000 });
  const names = plot.getByTestId("region-label");
  await expect(names).toHaveText(
    ["Supercritical", "Supercritical gas", "Supercritical liquid", "Liquid", "Gas", "Liquid + vapour"],
    { timeout: 10_000 },
  );
  const counts = await tinted(page);
  for (const phase of ["liquid", "twophase", "gas", "supercritical-liquid", "supercritical", "supercritical-gas"]) {
    expect(counts[phase] ?? 0, phase).toBeGreaterThan(2000);
  }
  await expect(page.getByText("Critical isotherm and isobar")).toBeVisible();
  await page.getByLabel("Tint the phase regions").uncheck();
  await expect(page).not.toHaveURL(/regions=1/);
  await expect(names).toHaveCount(0);
  await expect(plot).toHaveAttribute("data-regions", "off");
  expect(await tinted(page)).toEqual({});
});

test("a URL with the toggle opens tinted", async ({ page }) => {
  await ready(page, "./#/diagrams?fluid=n-Propane&d=pt&regions=1");
  await expect(page.getByLabel("Tint the phase regions")).toBeChecked();
  await expect(page.getByTestId("plot")).toHaveAttribute("data-regions", "ready", { timeout: 30_000 });
  // On p–T the liquid + vapour region is the vapour-pressure curve: five regions have room for a name.
  await expect(page.getByTestId("region-label")).toHaveCount(5);
});

// Their look, in each theme, as diagrams.spec.ts compares the diagrams'.
for (const id of ["ph", "ts"]) {
  for (const scheme of ["light", "dark"] as const) {
    test(`looks as it did: Water's ${id} with its regions, ${scheme}`, async ({ page }) => {
      test.skip(!process.env.CI, "compared on CI's runner, where the baselines were made");
      await page.setViewportSize({ width: 1280, height: 900 });
      await page.emulateMedia({ colorScheme: scheme, reducedMotion: "reduce" });
      await ready(page, `./#/diagrams?fluid=Water&d=${id}&regions=1`);
      await expect(page.getByTestId("plot")).toHaveAttribute("data-regions", "ready", { timeout: 30_000 });
      await expect(page.getByTestId("plot")).toHaveScreenshot(`water-${id}-regions-${scheme}.png`);
    });
  }
}

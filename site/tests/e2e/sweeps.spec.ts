// The sweeps and the benchmark in Chromium (PLAN.md M21.3): a sweep's lines drawn and read under the cursor as the
// package reads those states, its settings in the URL; the benchmark run on both builds, which agree bit for bit.
import { expect, test, type Page } from "@playwright/test";
import { propsSI, sameBits } from "../pk";

async function ready(page: Page, url: string): Promise<void> {
  await page.goto(url);
  await expect(page.getByTestId("sweep")).toHaveAttribute("data-state", "ready", { timeout: 30_000 });
}

test("draws n-Propane's c_p along four isobars and reads each under the cursor as the package does", async ({
  page,
}) => {
  await ready(page, "./#/sweeps");
  expect(await page.getByTestId("sweep-line").count()).toBeGreaterThanOrEqual(4);
  const box = await page.getByTestId("sweep").locator("svg").boundingBox();
  if (!box) throw new Error("no plot");
  await page.mouse.move(box.x + box.width * 0.5, box.y + box.height * 0.5);
  const cells = page.getByTestId("sweep-value");
  await expect(cells).toHaveCount(4);
  await expect(cells.first()).toHaveAttribute("data-value", /\d/);
  for (const cell of await cells.all()) {
    const inputs = JSON.parse((await cell.getAttribute("data-inputs")) ?? "null") as [string, number, string, number];
    const want = await propsSI("simd", "Cpmass", ...inputs, "n-Propane");
    expect(sameBits(Number(await cell.getAttribute("data-value")), want)).toBe(true);
  }
  await page.mouse.move(box.x + 2, box.y + 2);
  await expect(page.getByTestId("sweep-readout")).toContainText("Move over the plot");
});

test("takes held values in the shown units, and the latent heat along saturation, all in the URL", async ({ page }) => {
  await ready(page, "./#/sweeps?fluid=Water&units=engineering");
  await page.getByTestId("held").fill("20 47");
  await page.getByTestId("held").press("Enter");
  await expect(page).toHaveURL(/at=2000000%2C4700000/);
  await expect(page.getByTestId("sweep")).toHaveAttribute("data-state", "ready", { timeout: 30_000 });
  await expect(page.getByTestId("sweep-value")).toHaveCount(2);
  await page.getByRole("radio", { name: "Along saturation" }).check();
  await page.getByTestId("output").selectOption("latent_heat");
  await expect(page).toHaveURL(/mode=saturation&y=latent_heat/);
  await expect(page.getByTestId("held")).toHaveCount(0);
  await expect(page.getByTestId("sweep")).toHaveAttribute("data-state", "ready", { timeout: 30_000 });
  await expect(page.getByTestId("sweep-value")).toHaveCount(1);
});

test("runs the benchmark on both builds, which agree bit for bit", async ({ page }) => {
  await page.goto("./#/benchmark?fluid=n-Propane&pair=QT&n=1000");
  await expect(page.getByTestId("bench")).toHaveAttribute("data-state", "idle");
  await page.getByTestId("run").click();
  await expect(page.getByTestId("bench")).toHaveAttribute("data-state", "done", { timeout: 60_000 });
  await expect(page.getByTestId("bench-baseline")).toBeVisible();
  await expect(page.getByTestId("bench-simd128")).toBeVisible();
  await expect(page.getByTestId("comparison")).toHaveAttribute("data-differing", "0");
  await expect(page.getByTestId("comparison")).toContainText("agree bit for bit on all 4,000 cells");
  // New settings set the old run aside.
  await page.getByRole("radio", { name: "p, q" }).check();
  await expect(page).toHaveURL(/pair=PQ/);
  await expect(page.getByTestId("bench")).toHaveAttribute("data-state", "idle");
});

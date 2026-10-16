// The fluids beyond the five embedded, in Chromium (PLAN.md M21.4): the picker finds a fluid by name, alias or CAS
// number; its pack is fetched on its first use and only then, every worker gets it, and a reference the package embeds
// is not fetched (every ECS reference is one of the five, so no fluid fetches two packs here: tests/unit/packs.test.ts
// checks a reference's pack comes first where the registry lacks it); the page says what it fetched and decoded.
import { expect, test, type Page } from "@playwright/test";

/** The packs the page fetches from now on, by file, in order. */
function packsFetched(page: Page): string[] {
  const files: string[] = [];
  page.on("request", (r) => {
    const file = /\/packs\/([^/]+\.pack)$/.exec(new URL(r.url()).pathname)?.[1];
    if (file) files.push(file);
  });
  return files;
}

const T = (page: Page) => page.getByTestId("value-T");

test("a fluid's pack is fetched on its first use, and only its own: its reference is embedded", async ({ page }) => {
  const packs = packsFetched(page);
  await page.goto("./#/calculator?fluid=Water&a=P:101325&b=Q:0");
  await expect(T(page)).toHaveAttribute("data-value", /^373\.12/);
  await expect(page.getByTestId("fluid-origin")).toHaveText("Water: embedded in the package.");
  await expect(page.getByTestId("fluid-summary")).toHaveText(
    "This page has fetched 0 packs, 0 B of 8.99 MB, and decoded Water.",
  );
  const picker = page.getByRole("combobox", { name: "Fluid" });
  await picker.fill("75-10-5");
  const option = page.getByRole("option", { name: /^R32\b/ });
  await expect(option).toContainText("75-10-5");
  expect(packs).toEqual([]);
  await option.click();
  await expect(page).toHaveURL(/fluid=R32/);
  await expect(picker).toHaveValue("R32");
  await expect(T(page)).toHaveAttribute("data-value", /^221\.[45]/);
  expect(packs).toEqual(["r32.pack"]);
  await expect(page.getByTestId("fluid-origin")).toHaveText(
    /^R32: fetched, [\d.]+ kB; its transport reads n-Propane, embedded in the package\.$/,
  );
  // R32's conductivity reads n-Propane, so the registry has decoded it too.
  await expect(page.getByTestId("fluid-summary")).toHaveText(
    /^This page has fetched 1 pack, [\d.]+ kB of 8\.99 MB, and decoded Water, n-Propane, R32\.$/,
  );
});

test("an alias of an embedded fluid fetches nothing; the keys pick from the list", async ({ page }) => {
  const packs = packsFetched(page);
  await page.goto("./#/calculator?fluid=Water&a=T:300&b=P:101325");
  await expect(T(page)).toHaveAttribute("data-value", "300");
  const picker = page.getByRole("combobox", { name: "Fluid" });
  await picker.fill("r744");
  await expect(page.getByRole("option").first()).toContainText("CarbonDioxide");
  await picker.press("Enter");
  await expect(page).toHaveURL(/fluid=CarbonDioxide/);
  await expect(picker).toHaveValue("CarbonDioxide");
  await expect(page.getByTestId("value-molar_mass")).toHaveAttribute("data-value", /^0\.0440098/);
  await picker.focus();
  await picker.press("ArrowDown");
  await picker.press("Escape");
  await expect(picker).toHaveValue("CarbonDioxide");
  expect(packs).toEqual([]);
});

test("a fetched fluid's diagram draws, and the benchmark's workers each get its pack", async ({ page }) => {
  const packs = packsFetched(page);
  await page.goto("./#/diagrams?fluid=R32&d=ph");
  await expect(page.getByTestId("plot")).toHaveAttribute("data-state", "ready", { timeout: 30_000 });
  expect(packs).toEqual(["r32.pack"]);
  await page.goto("./#/benchmark?fluid=R32&pair=QT&n=1000");
  await page.getByTestId("run").click();
  await expect(page.getByTestId("bench")).toHaveAttribute("data-state", "done", { timeout: 60_000 });
  await expect(page.getByTestId("comparison")).toContainText("agree bit for bit on all 4,000 cells");
});

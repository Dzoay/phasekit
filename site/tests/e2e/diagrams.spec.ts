// The diagrams in Chromium (PLAN.md M21.2): drawn, read under the cursor as the package reads that state, toggled,
// switched, exported; and each diagram's look, against baselines made on CI's runner.
import { expect, test, type Page } from "@playwright/test";
import { readFileSync } from "node:fs";
import { propsSI, sameBits } from "../pk";

async function ready(page: Page, url: string): Promise<void> {
  await page.goto(url);
  await expect(page.getByTestId("plot")).toHaveAttribute("data-state", "ready", { timeout: 30_000 });
}

test("draws Water's p–h diagram with its families, ticks and titles", async ({ page }) => {
  await ready(page, "./#/diagrams?fluid=Water&d=ph");
  const plot = page.getByTestId("plot");
  await expect(plot.locator("canvas")).toBeVisible();
  await expect(plot.locator("text.title")).toHaveText(["h / J/kg", "p / Pa"]);
  expect(await plot.locator("text.tick-text").count()).toBeGreaterThan(6);
  expect(await plot.locator("text.label").count()).toBeGreaterThan(5);
  for (const name of ["Isotherms", "Isentropes", "Isochores", "Lines of constant quality"]) {
    await expect(page.getByLabel(name)).toBeChecked();
  }
});

test("reads the state under the cursor as the package does", async ({ page }) => {
  await ready(page, "./#/diagrams?fluid=Water&d=ph");
  const box = await page.getByTestId("plot").locator("svg").first().boundingBox();
  if (!box) throw new Error("no plot");
  await page.mouse.move(box.x + box.width * 0.45, box.y + box.height * 0.55);
  const cell = page.getByTestId("readout-T");
  await expect(cell).toHaveAttribute("data-value", /\d/);
  const inputs = JSON.parse((await page.getByTestId("readout").getAttribute("data-inputs")) ?? "null") as [
    string,
    number,
    string,
    number,
  ];
  expect(inputs[0]).toBe("Hmass");
  const want = await propsSI("simd", "T", inputs[0], inputs[1], inputs[2], inputs[3], "Water");
  expect(sameBits(Number(await cell.getAttribute("data-value")), want)).toBe(true);
  await page.mouse.move(box.x + 2, box.y + 2);
  await expect(page.getByTestId("readout")).toContainText("Move over the plot");
});

test("leads the side panel with the readout, which keeps its height as the cursor comes and goes", async ({ page }) => {
  await ready(page, "./#/diagrams?fluid=Water&d=ph");
  await expect(page.locator("aside.side > section").first()).toHaveAttribute("data-testid", "readout");
  const lines = page.locator("section.legend");
  const at = async () => (await lines.boundingBox())?.y;
  const before = await at();
  const box = await page.getByTestId("plot").locator("svg").first().boundingBox();
  if (!box) throw new Error("no plot");
  await page.mouse.move(box.x + box.width * 0.45, box.y + box.height * 0.55);
  await expect(page.getByTestId("readout-T")).toHaveAttribute("data-value", /\d/);
  expect(await at()).toBe(before);
  await page.mouse.move(box.x + 2, box.y + 2);
  await expect(page.getByTestId("readout")).toContainText("Move over the plot");
  await expect(page.getByTestId("readout-T")).not.toHaveAttribute("data-value");
  expect(await at()).toBe(before);
});

test("toggles families, switches diagrams and fluids, all in the URL", async ({ page }) => {
  await ready(page, "./#/diagrams?fluid=Water&d=ph");
  const labels = page.getByTestId("plot").locator("text.label");
  const before = await labels.count();
  await page.getByLabel("Isotherms").uncheck();
  await expect(page).toHaveURL(/hide=T/);
  await expect.poll(() => labels.count()).toBeLessThan(before);
  await page.getByRole("radio", { name: "T–s" }).check();
  await expect(page).toHaveURL(/d=ts/);
  await expect(page.getByTestId("plot")).toHaveAttribute("data-state", "ready", { timeout: 30_000 });
  await expect(page.getByTestId("plot").locator("text.title")).toHaveText(["s / J/(kg·K)", "T / K"]);
  await page.getByRole("combobox", { name: "Fluid" }).fill("nitrogen");
  await page.getByRole("option", { name: /^Nitrogen\b/ }).click();
  await expect(page).toHaveURL(/fluid=Nitrogen/);
  await expect(page.getByTestId("plot")).toHaveAttribute("data-state", "ready", { timeout: 30_000 });
});

test("exports the diagram as CSV, SVG and PNG", async ({ page }) => {
  await ready(page, "./#/diagrams?fluid=Water&d=ts");
  const save = async (name: string) => {
    const [download] = await Promise.all([page.waitForEvent("download"), page.getByRole("button", { name }).click()]);
    const path = await download.path();
    return { file: download.suggestedFilename(), bytes: readFileSync(path) };
  };
  const csv = await save("CSV");
  expect(csv.file).toBe("Water-ts.csv");
  expect(csv.bytes.toString("utf8").split("\n")[0]).toBe("line,value,segment,vertex,Smass,T");
  const svg = await save("SVG");
  expect(svg.file).toBe("Water-ts.svg");
  expect(svg.bytes.toString("utf8")).toMatch(/^<svg xmlns="http:\/\/www.w3.org\/2000\/svg"/);
  const png = await save("PNG");
  expect(png.file).toBe("Water-ts.png");
  expect([...png.bytes.subarray(0, 4)]).toEqual([0x89, 0x50, 0x4e, 0x47]);
});

test("shows a refused fluid in the package's words", async ({ page }) => {
  await page.goto("./#/diagrams?fluid=Unobtainium&d=ph");
  await expect(page.getByRole("alert")).toContainText("Unobtainium", { timeout: 30_000 });
});

// The look of each diagram, in each theme. Font rendering differs between machines, so the baselines are the CI
// runner's (made by its first run, PLAN.md M21.2) and the comparison runs there only.
for (const id of ["ph", "ts", "pt", "trho", "hs"]) {
  for (const scheme of ["light", "dark"] as const) {
    test(`looks as it did: Water's ${id}, ${scheme}`, async ({ page }) => {
      test.skip(!process.env.CI, "compared on CI's runner, where the baselines were made");
      await page.setViewportSize({ width: 1280, height: 900 });
      await page.emulateMedia({ colorScheme: scheme, reducedMotion: "reduce" });
      await ready(page, `./#/diagrams?fluid=Water&d=${id}`);
      await expect(page.getByTestId("plot")).toHaveScreenshot(`water-${id}-${scheme}.png`);
    });
  }
}

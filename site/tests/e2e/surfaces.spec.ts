// The surfaces in Chromium (PLAN.md M26.2): Water's p–v–T and u–s–v surfaces drawn in WebGL from the package's mesh,
// turned with the mouse, reached from the header, and a mixture's refused in the package's words.
import { expect, test, type Page } from "@playwright/test";
import { propsSI, sameBits } from "../pk";

async function ready(page: Page, url: string): Promise<void> {
  await page.goto(url);
  await expect(page.getByTestId("surface")).toHaveAttribute("data-state", "ready", { timeout: 30_000 });
}

/** The canvas's pixels as the page shows them, and how many are drawn (not transparent). */
async function picture(page: Page): Promise<{ drawn: number; data: string }> {
  return page
    .getByTestId("surface")
    .locator("canvas")
    .evaluate((c: HTMLCanvasElement) => {
      const copy = document.createElement("canvas");
      [copy.width, copy.height] = [c.width, c.height];
      const context = copy.getContext("2d") as CanvasRenderingContext2D;
      context.drawImage(c, 0, 0);
      const pixels = context.getImageData(0, 0, c.width, c.height).data;
      let drawn = 0;
      for (let i = 3; i < pixels.length; i += 4) if ((pixels[i] as number) > 0) drawn += 1;
      return { drawn, data: copy.toDataURL() };
    });
}

test("draws Water's p–v–T surface from the package's mesh, its axes and phases beside it", async ({ page }) => {
  await ready(page, "./#/surfaces?fluid=Water");
  const canvas = page.getByTestId("surface").locator("canvas");
  await expect(canvas).toHaveAttribute("aria-label", "The p–v–T surface of Water");
  expect(Number(await canvas.getAttribute("data-vertices"))).toBeGreaterThan(1000);
  const { drawn } = await picture(page);
  const box = await canvas.boundingBox();
  if (!box) throw new Error("no canvas");
  // The surface covers a good part of the view.
  expect(drawn).toBeGreaterThan(0.1 * box.width * box.height);
  const axes = page.getByTestId("axes");
  await expect(axes.getByRole("rowheader")).toHaveText(["Across", "Up", "Deep"]);
  await expect(axes.getByRole("row", { name: /Up/ })).toContainText("Pressure p, logarithmic");
  for (const name of ["Liquid", "Liquid + vapour", "Gas", "The saturation dome and critical point"]) {
    await expect(page.getByText(name, { exact: true })).toBeVisible();
  }
});

test("switches to Gibbs's u–s–v surface and writes it in the URL", async ({ page }) => {
  await ready(page, "./#/surfaces?fluid=Water");
  const before = await picture(page);
  await page.getByText("u–s–v", { exact: true }).click();
  await expect(page).toHaveURL(/#\/surfaces\?fluid=Water&kind=usv$/);
  await expect(page.getByTestId("surface").locator("canvas")).toHaveAttribute(
    "aria-label",
    "The u–s–v surface of Water",
  );
  await expect(page.getByTestId("axes").getByRole("row", { name: /Up/ })).toContainText("Internal energy u");
  await expect.poll(async () => (await picture(page)).data).not.toBe(before.data);
});

test("turns with a drag, and the reset brings the first view back", async ({ page }) => {
  await ready(page, "./#/surfaces?fluid=Water");
  const canvas = page.getByTestId("surface").locator("canvas");
  const first = await picture(page);
  const box = await canvas.boundingBox();
  if (!box) throw new Error("no canvas");
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await page.mouse.down();
  await page.mouse.move(box.x + box.width / 2 + 120, box.y + box.height / 2 + 30, { steps: 8 });
  await page.mouse.up();
  await expect.poll(async () => (await picture(page)).data).not.toBe(first.data);
  await page.getByRole("button", { name: "Reset view" }).click();
  await expect.poll(async () => (await picture(page)).data).toBe(first.data);
});

test("is reached from the header", async ({ page }) => {
  await page.goto("./#/calculator");
  await page.getByRole("navigation", { name: "Pages" }).getByRole("link", { name: "Surfaces" }).click();
  await expect(page).toHaveURL(/#\/surfaces\?fluid=Water$/);
  await expect(page.getByTestId("surface")).toHaveAttribute("data-state", "ready", { timeout: 30_000 });
});

test("refuses a mixture's surface in the package's words", async ({ page }) => {
  await page.goto("./#/surfaces?fluid=R410A.mix");
  const plot = page.getByTestId("surface");
  await expect(plot).toHaveAttribute("data-state", "refused", { timeout: 30_000 });
  await expect(plot.getByRole("alert")).toHaveText(/is a mixture: its surface is not drawn/);
});

test("labels its axes with their values and titles", async ({ page }) => {
  await ready(page, "./#/surfaces?fluid=Water");
  const labels = page.getByTestId("surface").locator(".labels");
  await expect(labels.locator(".title")).toHaveText(["v / m³/kg", "p / Pa", "T / K"]);
  expect(await labels.locator(".tick").count()).toBeGreaterThan(10);
  const texts = await labels.locator(".tick").allTextContents();
  // T's 1000 K, and p's decades as powers of ten.
  expect(texts).toContain("1000");
  expect(texts).toContain("10⁶");
  await page.getByText("u–s–v", { exact: true }).click();
  await expect(labels.locator(".title")).toHaveText(["s / J/(kg·K)", "u / J/kg", "v / m³/kg"]);
});

test("reads the state under the cursor as the package does, and none once it leaves", async ({ page }) => {
  await ready(page, "./#/surfaces?fluid=Water");
  const canvas = page.getByTestId("surface").locator("canvas");
  // The canvas reaches below the fold of the test's viewport.
  await canvas.scrollIntoViewIfNeeded();
  const box = await canvas.boundingBox();
  if (!box) throw new Error("no canvas");
  await page.mouse.move(box.x + box.width * 0.48, box.y + box.height * 0.62);
  const cell = page.getByTestId("readout-T");
  await expect(cell).toHaveAttribute("data-value", /\d/);
  const [t, rho] = JSON.parse((await page.getByTestId("readout").getAttribute("data-inputs")) ?? "null") as [
    number,
    number,
  ];
  for (const key of ["P", "Umass", "Smass"]) {
    const got = Number(await page.getByTestId(`readout-${key}`).getAttribute("data-value"));
    expect(sameBits(got, await propsSI("simd", key, "T", t, "Dmass", rho, "Water"))).toBe(true);
  }
  await expect(page.getByTestId("readout-phase")).toHaveText(
    /^(Liquid|Liquid \+ vapour|Gas|Supercritical liquid|Supercritical|Supercritical gas)$/,
  );
  await page.mouse.move(box.x + 4, box.y + 4);
  await expect(page.getByTestId("readout")).toContainText("Move over the surface.");
});

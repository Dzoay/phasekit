// Picking a state on a diagram (PLAN.md M21.8): the cursor snaps to round values, a click pins the readout, the arrow
// keys step it along the grid, and the calculator opens at the same state.
import { expect, test, type Page } from "@playwright/test";

async function ready(page: Page, url: string): Promise<void> {
  await page.goto(url);
  await expect(page.getByTestId("plot")).toHaveAttribute("data-state", "ready", { timeout: 30_000 });
}

const inputs = async (page: Page) =>
  JSON.parse((await page.getByTestId("readout").getAttribute("data-inputs")) ?? "null") as [
    string,
    number,
    string,
    number,
  ];

async function centre(page: Page) {
  const box = await page.getByTestId("plot").locator("svg").first().boundingBox();
  if (!box) throw new Error("no plot");
  return { x: box.x + box.width * 0.45, y: box.y + box.height * 0.55, box };
}

test("snaps the cursor to round values on the grid", async ({ page }) => {
  await ready(page, "./#/diagrams?fluid=Water&d=ph&units=engineering&snap=1&grid=2");
  await expect(page.getByTestId("grid-description")).toContainText("p: 1, 2, 5 per decade");
  const { x, y } = await centre(page);
  await page.mouse.move(x, y);
  await expect(page.getByTestId("readout-T")).toHaveAttribute("data-value", /\d/);
  const [k1, h, k2, p] = await inputs(page);
  expect([k1, k2]).toEqual(["Hmass", "P"]);
  // h on a step of 500 kJ/kg, p on 1, 2 or 5 × 10ⁿ bar: round values in J/kg and Pa too.
  expect(h % 500e3).toBe(0);
  const mantissa = p / 10 ** Math.floor(Math.log10(p));
  expect([1, 2, 5]).toContain(Number(mantissa.toPrecision(12)));
});

test("pins with a click, steps with the arrows, and opens the state in the calculator", async ({ page }) => {
  await ready(page, "./#/diagrams?fluid=Water&d=ph&units=engineering&snap=1&grid=2");
  const { x, y, box } = await centre(page);
  await page.mouse.move(x, y);
  await expect(page.getByTestId("readout-T")).toHaveAttribute("data-value", /\d/);
  await page.mouse.click(x, y);
  await expect(page.getByTestId("pinned")).toBeVisible();
  const pinned = await inputs(page);
  await page.mouse.move(box.x + box.width * 0.2, box.y + box.height * 0.3);
  await page.mouse.move(box.x + 2, box.y + 2);
  expect(await inputs(page)).toEqual(pinned);
  await page.keyboard.press("ArrowRight");
  await expect.poll(async () => (await inputs(page))[1]).toBe(pinned[1] + 500e3);
  expect((await inputs(page))[3]).toBe(pinned[3]);
  const t = await page.getByTestId("readout-T").getAttribute("data-value");
  await page.getByTestId("open-calculator").click();
  await expect(page).toHaveURL(/#\/calculator\?fluid=Water&a=Hmass/);
  await expect(page.getByTestId("value-T")).toHaveAttribute("data-value", t ?? "none");
});

test("lets a pinned readout go with Escape", async ({ page }) => {
  await ready(page, "./#/diagrams?fluid=Water&d=ph");
  const { x, y } = await centre(page);
  await page.mouse.move(x, y);
  await page.mouse.click(x, y);
  await expect(page.getByTestId("pinned")).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(page.getByTestId("pinned")).toHaveCount(0);
});

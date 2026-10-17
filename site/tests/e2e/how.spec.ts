// The how-it-works page in Chromium (PLAN.md M21.10): the property pipeline's four figures, reached from the header,
// each with its claim for screen readers, the Helmholtz model drawn in the page's accent, and only the figures wider
// than a phone.
import { expect, test } from "@playwright/test";

const PAIRS = "DT PT QT PQ HP PS PU HS SU DH DS DU HT ST TU DP QS HQ DQ".split(" ");

test("draws the pipeline's four figures, reached from the header", async ({ page }) => {
  await page.goto("./");
  await page.getByRole("link", { name: "How it works" }).click();
  await expect(page).toHaveURL(/#\/how-it-works$/);
  await expect(page.getByRole("link", { name: "How it works" })).toHaveAttribute("aria-current", "page");
  await expect(page.getByRole("heading", { level: 1 })).toHaveText("How a property is calculated");
  const figures = page.getByRole("main").getByRole("img");
  await expect(figures).toHaveCount(4);
  for (const figure of await figures.all()) {
    await expect(figure).toHaveAttribute("aria-label", /^\S+( \S+){9,}/);
  }
});

test("names each of the 19 input pairs once", async ({ page }) => {
  await page.goto("./#/how-it-works");
  const chips = await page.getByTestId("pairs").locator("text.chip-label").allTextContents();
  const named = chips.flatMap((chip) => chip.split(" "));
  expect([...named].sort()).toEqual([...PAIRS].sort());
});

test("draws the Helmholtz model in the page's accent", async ({ page }) => {
  await page.goto("./#/how-it-works");
  for (const accent of ["Teal", "Rose"]) {
    await page.getByRole("radio", { name: accent }).check();
    const [stroke, wanted] = await page
      .getByTestId("model")
      .first()
      .evaluate((el) => {
        const probe = document.createElement("span");
        probe.style.color = "var(--accent)";
        document.body.append(probe);
        const colour = getComputedStyle(probe).color;
        probe.remove();
        return [getComputedStyle(el).stroke, colour];
      });
    expect(stroke).toBe(wanted);
  }
});

test("is no wider than a phone: the figures scroll in their own frames", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto("./#/how-it-works");
  await expect(page.getByRole("heading", { level: 1 })).toBeVisible();
  const overflow = await page.evaluate(() => document.documentElement.scrollWidth - window.innerWidth);
  expect(overflow).toBeLessThanOrEqual(0);
  const frame = page.getByTestId("figure-frame").first();
  expect(await frame.evaluate((el) => el.scrollWidth > el.clientWidth)).toBe(true);
});

// The calculator in Chromium: the page shows the package's own values (`Exact`), from the build the worker chose.
import { expect, test } from "@playwright/test";
import { propsSI, sameBits } from "../pk";

test("shows the package's T at Water's normal boiling point", async ({ page }) => {
  await page.goto("./#/calculator?fluid=Water&a=P:101325&b=Q:0");
  const cell = page.getByTestId("value-T");
  await expect(cell).toHaveAttribute("data-value", /\d/);
  const shown = Number(await cell.getAttribute("data-value"));
  const want = await propsSI("simd", "T", "P", 101325, "Q", 0, "Water");
  expect(sameBits(shown, want), `${shown} vs ${want}`).toBe(true);
  await expect(page.getByTestId("phase")).toHaveText("twophase");
});

test("runs the simd128 build where the browser validates SIMD", async ({ page }) => {
  await page.goto("./");
  await expect(page.getByTestId("build")).toHaveText("simd128");
});

test("keeps its inputs in the URL", async ({ page }) => {
  await page.goto("./#/calculator?fluid=R134a&a=T:300&b=P:1e6");
  await expect(page.getByTestId("value-Dmass")).toHaveAttribute("data-value", /\d/);
  await page.getByLabel("Fluid").selectOption("Water");
  await expect(page).toHaveURL(/fluid=Water/);
  await expect(page).toHaveURL(/a=T(:|%3A)300/);
});

test("shows a refused state in the package's words", async ({ page }) => {
  await page.goto("./#/calculator?fluid=Water&a=T:300&b=P:-5");
  await expect(page.getByRole("alert")).toContainText(/\S/);
  await expect(page.getByTestId("value-T")).toHaveCount(0);
});

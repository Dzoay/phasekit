// The calculator under a viewer's hands: each control changes the view and the view stays changed, and a new URL
// (a link, the back button) still loads its view.
import { expect, test, type Page } from "@playwright/test";

const T = (page: Page) => page.getByTestId("value-T");

async function ready(page: Page, url: string): Promise<void> {
  await page.goto(url);
  await expect(T(page)).toHaveAttribute("data-value", /\d/);
}

test("changing the first input's variable keeps the state", async ({ page }) => {
  await ready(page, "./#/calculator?fluid=Water&a=P:101325&b=Q:0");
  const before = Number(await T(page).getAttribute("data-value"));
  const h = await page.getByTestId("value-Hmass").getAttribute("data-value");
  await page.getByLabel("First input variable").selectOption("Hmass");
  await expect(page.getByLabel("First input variable")).toHaveValue("Hmass");
  await expect(page).toHaveURL(new RegExp(`a=Hmass(:|%3A)${h}`));
  await expect(page.getByLabel("First input value")).toHaveValue(/^419057\.7/);
  await expect(page.getByTestId("value-P")).toHaveAttribute("data-value", /^10132[45]\./);
  expect(Math.abs(Number(await T(page).getAttribute("data-value")) - before)).toBeLessThan(1e-6);
});

test("changing the second input's variable keeps the state", async ({ page }) => {
  await ready(page, "./#/calculator?fluid=Water&a=T:300&b=P:101325");
  await page.getByLabel("Second input variable").selectOption("Dmass");
  await expect(page.getByLabel("Second input variable")).toHaveValue("Dmass");
  await expect(page).toHaveURL(/b=Dmass/);
  await expect(page.getByTestId("value-P")).toHaveAttribute("data-value", /^10132[45]/);
});

test("typing a value recomputes and keeps what was typed", async ({ page }) => {
  await ready(page, "./#/calculator?fluid=Water&a=T:300&b=P:101325");
  const value = page.getByLabel("Second input value");
  await value.fill("2e5");
  await expect(page).toHaveURL(/b=P(:|%3A)200000/);
  // The package recomputes p from the solved state, so it meets the input within `Flash` (1e-9, VERIFICATION.md §5).
  await expect(page.getByTestId("value-P")).toHaveAttribute("data-value", /^(199999\.99|200000)/);
  expect(Math.abs(Number(await page.getByTestId("value-P").getAttribute("data-value")) / 2e5 - 1)).toBeLessThan(1e-9);
  await expect(value).toHaveValue("2e5");
});

test("the units switch converts the inputs and the outputs", async ({ page }) => {
  await ready(page, "./#/calculator?fluid=Water&a=P:101325&b=Q:0");
  const si = await T(page).getAttribute("data-value");
  await page.getByRole("radio", { name: "°C, bar, kJ" }).check();
  await expect(page).toHaveURL(/units=engineering/);
  await expect(page.getByLabel("First input value")).toHaveValue("1.01325");
  await expect(T(page)).toContainText("99.974");
  await expect(T(page)).toHaveAttribute("data-value", si ?? "");
  await page.getByRole("radio", { name: "SI" }).check();
  await expect(page.getByLabel("First input value")).toHaveValue("101325");
});

test("a preset loads its fluid and inputs", async ({ page }) => {
  await ready(page, "./#/calculator?fluid=Water&a=P:101325&b=Q:0");
  await page.getByRole("button", { name: "Liquid nitrogen at 77 K" }).click();
  await expect(page.getByLabel("Fluid")).toHaveValue("Nitrogen");
  await expect(T(page)).toHaveAttribute("data-value", "77");
  await expect(page.getByTestId("phase")).toHaveText("liquid");
  await expect(page).toHaveURL(/fluid=Nitrogen/);
});

test("a new URL loads its view", async ({ page }) => {
  await ready(page, "./#/calculator?fluid=Water&a=P:101325&b=Q:0");
  await page.evaluate(() => (location.hash = "#/calculator?fluid=R134a&a=T:300&b=Q:1"));
  await expect(page.getByLabel("Fluid")).toHaveValue("R134a");
  await expect(T(page)).toHaveAttribute("data-value", "300");
});

test("a chosen fluid computes and stays chosen", async ({ page }) => {
  await ready(page, "./#/calculator?fluid=Water&a=T:300&b=P:100000");
  await page.getByLabel("Fluid").selectOption("Nitrogen");
  await expect(page.getByTestId("value-molar_mass")).toHaveAttribute("data-value", /^0\.0280134/);
  await page.getByRole("radio", { name: "°C, bar, kJ" }).check();
  await expect(page.getByLabel("Fluid")).toHaveValue("Nitrogen");
  await expect(page.getByTestId("value-molar_mass")).toHaveAttribute("data-value", /^0\.0280134/);
  await expect(page).toHaveURL(/fluid=Nitrogen/);
});

test("a refusal restates the inputs in SI and in the viewer's units", async ({ page }) => {
  // −273.15 °C is 0 K, which the package refuses; its message names the SI value, without a unit.
  await page.goto("./#/calculator?fluid=Water&a=P:1002500000&b=T:0&units=engineering");
  const alert = page.getByRole("alert");
  await expect(alert).toContainText("invalid temperature: 0");
  const inputs = page.getByTestId("refusal-inputs");
  await expect(inputs).toContainText("T = 0 K (−273.15 °C)");
  await expect(inputs).toContainText("p = 1.0025 × 10⁹ Pa (10025 bar)");
  await page.getByRole("radio", { name: "SI" }).check();
  await expect(inputs).toContainText("T = 0 K");
  await expect(inputs).not.toContainText("°C");
});

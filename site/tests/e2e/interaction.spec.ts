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
  await page.getByRole("combobox", { name: "Fluid" }).fill("nitrogen");
  await page
    .getByRole("listbox", { name: "Fluids" })
    .getByRole("option", { name: /^Nitrogen\b/ })
    .click();
  await expect(page.getByTestId("value-molar_mass")).toHaveAttribute("data-value", /^0\.0280134/);
  await page.getByRole("radio", { name: "°C, bar, kJ" }).check();
  await expect(page.getByLabel("Fluid")).toHaveValue("Nitrogen");
  await expect(page.getByTestId("value-molar_mass")).toHaveAttribute("data-value", /^0\.0280134/);
  await expect(page).toHaveURL(/fluid=Nitrogen/);
});

test("a refusal is the package's, written in the viewer's units", async ({ page }) => {
  // −273.15 °C is 0 K, which the package refuses; the page writes its message in the units shown (PLAN.md M28.3).
  await page.goto("./#/calculator?fluid=Water&a=P:1002500000&b=T:0&units=engineering");
  const message = page.getByTestId("refusal-message");
  await expect(message).toHaveText("temperature −273.15 °C is not above absolute zero");
  await page.getByRole("radio", { name: "SI" }).check();
  await expect(message).toHaveText("temperature 0 K is not above absolute zero");
});

test("a refusal offers its remedy, and an unknown fluid the names nearest it", async ({ page }) => {
  // PLAN.md M29.7: the package's remedies in the site's terms, and "did you mean" for a fluid the registry lacks.
  await page.goto("./#/calculator?fluid=Water&a=P:100000&b=T:3000");
  await expect(page.getByTestId("refusal-message")).toHaveText("temperature 3000 K is above Water's maximum, 2000 K");
  await expect(page.getByTestId("remedy")).toContainText("does not extrapolate");
  await page.goto("./#/calculator?fluid=Watr&a=P:100000&b=T:300");
  const nearest = page.getByTestId("did-you-mean");
  await expect(nearest).toContainText("Did you mean");
  await nearest.getByRole("button", { name: "Water" }).click();
  await expect(page.getByTestId("phase")).toHaveText("liquid");
  await expect(page).toHaveURL(/fluid=Water&/);
});

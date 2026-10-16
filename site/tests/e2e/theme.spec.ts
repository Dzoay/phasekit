// The themes in Chromium: a choice survives a reload, and the page works where storage is blocked.
import { expect, test } from "@playwright/test";

const scheme = () => getComputedStyle(document.documentElement).colorScheme;
const background = () => getComputedStyle(document.body).backgroundColor;

test("the chosen theme survives a reload", async ({ page }) => {
  await page.emulateMedia({ colorScheme: "light" });
  await page.goto("./");
  const light = await page.evaluate(background);
  await page.getByRole("radio", { name: "Dark" }).check();
  await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");
  expect(await page.evaluate(scheme)).toBe("dark");
  expect(await page.evaluate(background)).not.toBe(light);
  await page.getByRole("radio", { name: "Amber" }).check();
  await page.reload();
  await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");
  await expect(page.locator("html")).toHaveAttribute("data-accent", "amber");
  expect(await page.evaluate(scheme)).toBe("dark");
});

test("the system theme follows the browser", async ({ page }) => {
  await page.emulateMedia({ colorScheme: "dark" });
  await page.goto("./");
  const dark = await page.evaluate(background);
  await page.emulateMedia({ colorScheme: "light" });
  expect(await page.evaluate(background)).not.toBe(dark);
});

test("the page renders with storage blocked", async ({ page }) => {
  await page.addInitScript(() => {
    Object.defineProperty(window, "localStorage", {
      get() {
        throw new DOMException("The operation is insecure.", "SecurityError");
      },
    });
  });
  await page.goto("./#/calculator?fluid=Water&a=P:101325&b=Q:0");
  await expect(page.getByTestId("value-T")).toHaveAttribute("data-value", /\d/);
  await page.getByRole("radio", { name: "Dark" }).check();
  await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");
});

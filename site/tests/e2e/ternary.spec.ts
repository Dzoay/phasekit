// The ternary diagrams in Chromium (PLAN.md M26.6): R407C's components at 273.15 K and 5 bar, where R407C splits, its
// two-phase region, its bubble and dew sides and its tie lines from the package, R407C's own composition inside the
// region; a new pressure in the URL; reached from the header; and a fluid of no three components refused.
import { expect, test, type Page } from "@playwright/test";

async function ready(page: Page, url: string): Promise<void> {
  await page.goto(url);
  await expect(page.locator(".plot")).toHaveAttribute("data-state", "ready", { timeout: 60_000 });
}

test("draws R407C's components' two-phase region, its two sides and tie lines, R407C inside it", async ({ page }) => {
  await ready(page, "./#/ternary?mixture=R407C.mix&T=273.15&p=500000");
  const svg = page.getByTestId("ternary");
  expect(Number(await svg.getAttribute("data-pieces"))).toBeGreaterThan(50);
  await expect(svg).toHaveAttribute("data-lines", "2");
  expect(Number(await svg.getAttribute("data-ties"))).toBeGreaterThan(4);
  await expect(svg.locator("path.bubble")).toHaveCount(1);
  await expect(svg.locator("path.dew")).toHaveCount(1);
  for (const name of ["R32", "R125", "R134a"]) await expect(svg.getByText(name, { exact: true })).toBeVisible();
  // R407C's own composition lies on a piece of the region, as its flash at (T, p) splits it. The viewport is 720 px
  // high: the point is brought into it first.
  await svg.getByTestId("feed").scrollIntoViewIfNeeded();
  const feed = await svg.getByTestId("feed").boundingBox();
  if (!feed) throw new Error("no feed");
  const under = await page.evaluate(
    ([x, y]) => document.elementsFromPoint(x as number, y as number).map((e) => e.getAttribute("class") ?? ""),
    [feed.x + feed.width / 2, feed.y + feed.height / 2],
  );
  expect(under.some((c) => c.includes("region"))).toBe(true);
});

test("takes a new pressure into the URL, and reads the composition under the pointer", async ({ page }) => {
  await ready(page, "./#/ternary?mixture=R407C.mix&T=273.15&p=500000");
  const before = await page.getByTestId("ternary").getAttribute("data-pieces");
  await page.getByLabel("p / kPa").fill("400");
  await page.getByLabel("p / kPa").press("Enter");
  await expect(page).toHaveURL(/p=400000/);
  await expect(page.locator(".plot")).toHaveAttribute("data-state", "ready", { timeout: 60_000 });
  await expect(page.getByTestId("ternary")).not.toHaveAttribute("data-pieces", before ?? "");
  await page.locator(".cursor").scrollIntoViewIfNeeded();
  const box = await page.getByTestId("ternary").boundingBox();
  if (!box) throw new Error("no triangle");
  await page.mouse.move(box.x + box.width / 2, box.y + box.height - 60);
  await expect(page.locator(".cursor")).toContainText(/R32 [\d.]+ · R125 [\d.]+ · R134a [\d.]+/);
});

test("is reached from the header", async ({ page }) => {
  await page.goto("./#/calculator");
  await page.getByRole("navigation", { name: "Pages" }).getByRole("link", { name: "Ternary" }).click();
  await expect(page.locator(".plot")).toHaveAttribute("data-state", "ready", { timeout: 60_000 });
  await expect(page).toHaveURL(/#\/ternary\?mixture=R407C\.mix&T=273\.15&p=500000$/);
});

test("refuses a fluid that is no three-component mixture", async ({ page }) => {
  await page.goto("./#/ternary?mixture=Water");
  await expect(page.locator(".plot")).toHaveAttribute("data-state", "refused", { timeout: 30_000 });
  await expect(page.getByRole("alert")).toContainText("three-component");
});

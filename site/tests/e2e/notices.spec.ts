// The licences page: every notice the build collects is served and shown.
import { expect, test } from "@playwright/test";

test("shows every notice the page ships", async ({ page }) => {
  await page.goto("./#/licences");
  const notices = page.locator("details[data-testid^='notice-']");
  await expect(notices).toHaveCount(6);
  for (const notice of await notices.all()) {
    await notice.locator("summary").click();
    await expect(notice.locator("pre")).toContainText(/licen[cs]e/i);
  }
});

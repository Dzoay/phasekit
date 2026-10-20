// Mixtures in Chromium (PLAN.md M21.5): R410A.mix's phase envelope on p–T and T–ρ, from its components' packs; a
// two-phase readout inside it, each phase's mole fractions the package's; a predefined mixture picked and written out
// to change its fractions, a pure fluid mixed with another; and a liquid-liquid split refused in the package's words.
import { expect, test, type Page } from "@playwright/test";
import { loadPk, packBytes, sameBits } from "../pk";

/** The packs the page fetches from now on, by file, in order. */
function packsFetched(page: Page): string[] {
  const files: string[] = [];
  page.on("request", (r) => {
    const file = /\/packs\/([^/]+\.pack)$/.exec(new URL(r.url()).pathname)?.[1];
    if (file) files.push(file);
  });
  return files;
}

async function ready(page: Page, url: string): Promise<void> {
  await page.goto(url);
  await expect(page.getByTestId("plot")).toHaveAttribute("data-state", "ready", { timeout: 30_000 });
}

/** The package's phases of R410A.mix at a state, from R32's and R125's packs, as the page's engine computes them. */
async function phases(inputs: [string, number, string, number]): Promise<{ liquid: number[]; vapour: number[] }> {
  const pk = await loadPk("simd");
  let registry = new pk.Registry();
  for (const file of ["r32.pack", "r125.pack"]) {
    const next = registry.withPack(packBytes(file));
    registry.free();
    registry = next;
  }
  const state = registry.fluid("R410A.mix").flash(...inputs);
  const split = state.phases();
  const got = { liquid: [...(split?.liquid ?? [])], vapour: [...(split?.vapour ?? [])] };
  split?.free();
  state.free();
  registry.free();
  return got;
}

test("R410A.mix's envelope draws on p–T and T–ρ from its components' packs, and a mixture has no other plane", async ({
  page,
}) => {
  const packs = packsFetched(page);
  await ready(page, "./#/diagrams?fluid=R410A.mix&d=pt");
  expect(packs).toEqual(["r32.pack", "r125.pack"]);
  await expect(page.getByTestId("envelope-end")).toHaveAttribute("data-end", "closed");
  await expect(page.getByTestId("plot").locator("text.title")).toHaveText(["T / K", "p / Pa"]);
  await expect(page.getByRole("radio", { name: "p–h" })).toBeDisabled();
  await page.getByRole("radio", { name: "T–ρ" }).check();
  await expect(page).toHaveURL(/d=trho/);
  await expect(page.getByTestId("plot")).toHaveAttribute("data-state", "ready", { timeout: 30_000 });
  await expect(page.getByTestId("plot").locator("text.title")).toHaveText(["ρ / kg/m³", "T / K"]);
  await expect(page.getByTestId("fluid-origin")).toHaveText(
    /^R410A\.mix: a mixture of R32, fetched, [\d.]+ kB; R125, fetched, [\d.]+ kB\.$/,
  );
});

test("reads a two-phase state inside the envelope, each phase's mole fractions the package's", async ({ page }) => {
  await ready(page, "./#/diagrams?fluid=R410A.mix&d=trho");
  const box = await page.getByTestId("plot").locator("svg").first().boundingBox();
  if (!box) throw new Error("no plot");
  await page.mouse.move(box.x + box.width * 0.5, box.y + box.height * 0.6);
  await expect(page.getByTestId("readout-phase")).toHaveText("twophase");
  const inputs = JSON.parse((await page.getByTestId("readout").getAttribute("data-inputs")) ?? "null") as [
    string,
    number,
    string,
    number,
  ];
  expect(inputs[0]).toBe("Dmass");
  const want = await phases(inputs);
  for (const [side, values] of Object.entries(want)) {
    for (const [i, v] of values.entries()) {
      const cell = page.getByTestId(`${side}-${i}`);
      await expect(cell).toHaveAttribute("data-value", /\d/);
      expect(sameBits(Number(await cell.getAttribute("data-value")), v), `${side} ${i}`).toBe(true);
    }
  }
  expect(want.liquid.length).toBe(2);
});

test("picks a predefined mixture, writes it out to change a fraction, and mixes a pure fluid", async ({ page }) => {
  await page.goto("./#/calculator?fluid=Water&a=T:300&b=P:101325");
  await expect(page.getByTestId("value-T")).toHaveAttribute("data-value", "300");
  const picker = page.getByRole("combobox", { name: "Fluid", exact: true });
  await picker.fill("R410A");
  const option = page.getByRole("listbox", { name: "Fluids" }).getByRole("option", { name: /^R410A\.mix\b/ });
  await expect(option).toContainText("R32 + R125");
  await option.click();
  await expect(page).toHaveURL(/fluid=R410A\.mix/);
  await expect(page.getByTestId("known-composition")).toHaveText(/^R32 0\.69761\d*, R125 0\.30238\d*$/);
  await page.getByRole("button", { name: "Change the fractions" }).click();
  await expect(picker).toHaveValue(/^R32\[0\.6976146993758\d*\]&R125\[0\.3023853006241\d*\]$/);
  await page.getByLabel("Mole fraction of component 1").fill("0.5");
  await page.getByLabel("Mole fraction of component 2").fill("0.5");
  await expect(picker).toHaveValue("R32[0.5]&R125[0.5]");
  await expect(page.getByTestId("mixture-composition").getByTestId("feed-0")).toHaveText("0.5");
  await page.getByLabel("Remove component 2").click();
  await expect(picker).toHaveValue("R32");
  await expect(page.getByTestId("mixture-composition")).toHaveCount(0);
  await page.getByLabel("Make a mixture with").selectOption("R125");
  await expect(picker).toHaveValue("R32[0.5]&R125[0.5]");
  await expect(page.getByTestId("mixture-composition")).toBeVisible();
});

test("shows a two-phase state's phases, and a liquid-liquid split refused in the package's words", async ({ page }) => {
  await page.goto("./#/calculator?fluid=R410A.mix&a=T:280&b=Q:0.5");
  const want = await phases(["T", 280, "Q", 0.5]);
  const liquid = page.getByTestId("mixture-composition").getByTestId("liquid-0");
  await expect(liquid).toHaveAttribute("data-value", String(want.liquid[0]));
  const lle = new URLSearchParams({ fluid: "Methanol[0.54]&Benzene[0.46]", a: "T:308.15", b: "P:101325" });
  await page.goto(`./#/calculator?${lle}`);
  await expect(page.getByRole("alert")).toContainText("input pair PT is not supported by this model");
  await expect(page.getByTestId("liquid-liquid")).toContainText("refusal of two liquid phases");
});

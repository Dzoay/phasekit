// The mixture editor's view (PLAN.md M21.5): rows of components and fractions read from the fluid's mixture name, each
// fraction written back as typed; a typed fraction that is not a number shown but not written; components renamed,
// removed and added among the fluids the library pairs with the rest; a pure fluid joined at 0.5 each; and a predefined
// mixture written out with the package's fractions.
import { describe, expect, test } from "vitest";
import { FluidData } from "../../src/lib/fluiddata.svelte";
import { MixtureEditor } from "../../src/lib/mixture.svelte";
import { pairKey } from "../../src/lib/mixtures";
import type { PackEntry } from "../../src/lib/packs";

const entry = (name: string, cas: string | null, aliases: string[] = []): PackEntry => ({
  name,
  aliases,
  cas,
  requires: [],
  pack: `${name.toLowerCase()}.pack`,
  bytes: 1,
});

// A pairs with B and C, B with C; D has no CAS number and pairs with nothing.
const [a, b, c, d] = [entry("A", "1-1-1", ["alpha"]), entry("B", "2-2-2"), entry("C", "3-3-3"), entry("D", null)];

function setup(fluid: string) {
  const data = new FluidData(
    { index: () => Promise.reject(new Error("unused")), pack: () => Promise.reject(new Error("unused")) },
    {
      add: () => Promise.reject(new Error("unused")),
      contents: () => Promise.reject(new Error("unused")),
      components: () => Promise.reject(new Error("unused")),
      mixtures: () => Promise.reject(new Error("unused")),
    },
  );
  data.fluids = [a, b, c, d];
  data.mixable = {
    predefined: [],
    pairs: [pairKey("1-1-1", "2-2-2"), pairKey("1-1-1", "3-3-3"), pairKey("2-2-2", "3-3-3")],
  };
  const box = $state({ fluid });
  const editor = new MixtureEditor(data, { get: () => box.fluid, set: (name) => (box.fluid = name) });
  return { editor, box };
}

describe("the mixture editor", () => {
  test("reads a mixture name's rows, each fraction as written; any other fluid has none", () => {
    const { editor, box } = setup("A[0.25]&B[7.5e-1]");
    expect(editor.parts).toEqual([
      { name: "A", fraction: "0.25" },
      { name: "B", fraction: "7.5e-1" },
    ]);
    expect([editor.fraction(0), editor.fraction(1), editor.fraction(2)]).toEqual(["0.25", "7.5e-1", ""]);
    box.fluid = "R410A.mix";
    expect([editor.parts, editor.fraction(0)]).toEqual([null, ""]);
  });

  test("writes a fraction as typed; shows one that is not a number without writing it, until the fluid changes", () => {
    const { editor, box } = setup("A[0.5]&B[0.5]");
    editor.type(0, "0.25");
    expect(box.fluid).toBe("A[0.25]&B[0.5]");
    editor.type(1, "0.5.");
    expect([box.fluid, editor.fraction(1)]).toEqual(["A[0.25]&B[0.5]", "0.5."]);
    editor.type(1, "");
    expect([box.fluid, editor.fraction(1), editor.fraction(0)]).toEqual(["A[0.25]&B[0.5]", "", "0.25"]);
    editor.type(1, "0.75");
    expect([box.fluid, editor.fraction(1)]).toEqual(["A[0.25]&B[0.75]", "0.75"]);
    editor.type(0, "x");
    expect(editor.fraction(0)).toBe("x");
    box.fluid = "A[0.1]&B[0.9]";
    expect(editor.fraction(0)).toBe("0.1");
  });

  test("renames and removes rows; one row left is its fluid alone; a pure fluid has no rows to change", () => {
    const { editor, box } = setup("A[0.5]&B[0.25]&C[0.25]");
    editor.rename(1, "D");
    expect(box.fluid).toBe("A[0.5]&D[0.25]&C[0.25]");
    editor.remove(1);
    expect(box.fluid).toBe("A[0.5]&C[0.25]");
    editor.remove(0);
    expect(box.fluid).toBe("C");
    for (const change of [() => editor.type(0, "1"), () => editor.rename(0, "A"), () => editor.remove(0)]) {
      change();
      expect(box.fluid).toBe("C");
    }
  });

  test("adds a component to a mixture at 0, and to a pure fluid at 0.5 each, under its canonical name", () => {
    const mixed = setup("A[0.5]&B[0.5]");
    mixed.editor.add("C");
    expect(mixed.box.fluid).toBe("A[0.5]&B[0.5]&C[0]");
    const pure = setup("alpha");
    pure.editor.add("B");
    expect(pure.box.fluid).toBe("A[0.5]&B[0.5]");
    const unknown = setup("Unobtainium");
    unknown.editor.add("B");
    expect(unknown.box.fluid).toBe("Unobtainium[0.5]&B[0.5]");
  });

  test("offers each row the fluids that pair with every other row's, and to add those that pair with all", () => {
    const { editor, box } = setup("A[0.5]&B[0.5]");
    expect(editor.options(0)).toEqual([a, c]);
    expect(editor.options(1)).toEqual([b, c]);
    expect(editor.options()).toEqual([c]);
    box.fluid = "alpha";
    expect(editor.options()).toEqual([b, c]);
    box.fluid = "D";
    expect(editor.options()).toEqual([]);
    box.fluid = "R410A.mix";
    expect(editor.options()).toEqual([]);
    box.fluid = "A[0.5]&Gone[0.5]";
    expect([editor.options(0), editor.options(1)]).toEqual([[], [b, c]]);
    box.fluid = "A[0.4]&B[0.3]&Gone[0.3]";
    expect([editor.options(), editor.options(2)]).toEqual([[], [c]]);
  });

  test("writes a predefined mixture out with the package's fractions, each to its last digit", () => {
    const { editor, box } = setup("AB.mix");
    editor.type(0, "x");
    editor.edit(["A", "B"], [0.6976146993758624, 0.30238530062413754]);
    expect(box.fluid).toBe("A[0.6976146993758624]&B[0.30238530062413754]");
    expect(editor.fraction(0)).toBe("0.6976146993758624");
  });
});

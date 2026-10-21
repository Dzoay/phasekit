// The five diagrams (PLAN.md M21.2): their axes, the isoline families each draws, each family's sweep, and the input
// pair the cursor's readout flashes. At most three coloured families beside the lines of constant quality, the most the
// isoline palette keeps apart for colour-blind readers when every pair can sit side by side (the dataviz skill). The
// package draws them (`Fluid.diagram`, M21.6): the page needs its tabs before the engine has loaded, so it keeps this
// table, and Vitest holds the axes and families to the package's `planes()`.

/** A state variable, by the package's input key. */
export type Key = "T" | "P" | "Hmass" | "Smass" | "Dmass" | "Q";

export type DiagramId = "ph" | "ts" | "pt" | "trho" | "hs";

export interface Axis {
  key: Key;
  log: boolean;
}

/** An isoline family: the variable each line holds, and the one it sweeps across the diagram. */
export interface Family {
  held: Key;
  sweep: "T" | "P";
}

export interface Diagram {
  id: DiagramId;
  title: string;
  x: Axis;
  y: Axis;
  families: readonly Family[];
  readout: readonly [Key, Key];
}

const quality: Family = { held: "Q", sweep: "T" };

export const DIAGRAMS: readonly Diagram[] = [
  {
    id: "ph",
    title: "p–h",
    x: { key: "Hmass", log: false },
    y: { key: "P", log: true },
    families: [{ held: "T", sweep: "P" }, { held: "Smass", sweep: "P" }, { held: "Dmass", sweep: "P" }, quality],
    readout: ["Hmass", "P"],
  },
  {
    id: "ts",
    title: "T–s",
    x: { key: "Smass", log: false },
    y: { key: "T", log: false },
    families: [{ held: "P", sweep: "T" }, { held: "Hmass", sweep: "T" }, { held: "Dmass", sweep: "T" }, quality],
    readout: ["Smass", "T"],
  },
  {
    id: "pt",
    title: "p–T",
    x: { key: "T", log: false },
    y: { key: "P", log: true },
    families: [{ held: "Dmass", sweep: "T" }],
    readout: ["T", "P"],
  },
  {
    id: "trho",
    title: "T–ρ",
    x: { key: "Dmass", log: true },
    y: { key: "T", log: false },
    families: [{ held: "P", sweep: "T" }, quality],
    readout: ["Dmass", "T"],
  },
  {
    id: "hs",
    title: "h–s",
    x: { key: "Smass", log: false },
    y: { key: "Hmass", log: false },
    families: [{ held: "P", sweep: "T" }, { held: "T", sweep: "P" }, quality],
    readout: ["Hmass", "Smass"],
  },
];

export function diagram(id: string): Diagram | undefined {
  return DIAGRAMS.find((d) => d.id === id);
}

/** The diagrams a mixture has: the planes of its envelope (PLAN.md M21.5); the package refuses the others. */
export const MIXTURE_DIAGRAMS: readonly DiagramId[] = ["pt", "trho"];

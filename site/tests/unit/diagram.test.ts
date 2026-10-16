// How a diagram is drawn (PLAN.md M21.2): its families' styles and labels, its scales and the readout's inputs, the
// drawing model the canvas and the SVG export share, and the CSV of its vertices.
import { scaleLinear } from "d3-scale";
import { describe, expect, test } from "vitest";
import {
  buildModel,
  decadeText,
  overlaps,
  place,
  renderSvg,
  toCsv,
  pathOf,
  type Box,
  type Model,
} from "../../src/diagram/model";
import { padded, baseScales, readoutInputs } from "../../src/diagram/scales";
import { lineLabel, styleOf } from "../../src/diagram/style";
import { DIAGRAMS, diagram, type Diagram } from "../../src/sample/diagrams";
import type { DiagramData, Vertex } from "../../src/sample/lines";

const ph = diagram("ph") as Diagram;
const v = (x: number, y: number): Vertex => ({ x, y, inputs: ["T", 300, "P", y] });

const data: DiagramData = {
  dome: {
    liquid: [v(100e3, 1e3), v(400e3, 1e5)],
    vapour: [v(2500e3, 1e3), v(2600e3, 1e5)],
    critical: v(2000e3, 2.2e7),
  },
  lines: [
    { held: "T", value: 400, segments: [{ twoPhase: false, vertices: [v(2700e3, 1e3), v(2650e3, 1e5)] }] },
    { held: "Smass", value: 6000, segments: [{ twoPhase: true, vertices: [v(1500e3, 1e4), v(1700e3, 1e5)] }] },
    { held: "Q", value: 0.5, segments: [{ twoPhase: true, vertices: [v(1300e3, 1e3), v(1400e3, 1e5)] }] },
  ],
  x: [0, 3000e3],
  y: [1e3, 1e8],
};

const colours = (token: string) => `rgb(${token})`;

describe("styles", () => {
  test("families that share a diagram never share a colour; a colour keeps its dash", () => {
    for (const d of DIAGRAMS) {
      const coloured = d.families.filter((f) => f.held !== "Q").map((f) => styleOf(f.held).colour);
      expect(new Set(coloured).size, d.id).toBe(coloured.length);
    }
    const dashes = new Map<string, string>();
    for (const held of ["T", "P", "Smass", "Hmass", "Dmass"] as const) {
      const s = styleOf(held);
      expect(dashes.get(s.colour) ?? JSON.stringify(s.dash), held).toBe(JSON.stringify(s.dash));
      dashes.set(s.colour, JSON.stringify(s.dash));
      expect(s.width).toBe(1.5);
    }
    expect([...dashes.keys()].sort()).toEqual(["--iso-1", "--iso-2", "--iso-3"]);
    expect(styleOf("Q")).toEqual({ name: "Lines of constant quality", colour: "--fg-muted", dash: [2, 3], width: 1 });
    expect(["T", "P", "Smass", "Hmass", "Dmass"].map((k) => styleOf(k as "T").name)).toEqual([
      "Isotherms",
      "Isobars",
      "Isentropes",
      "Isenthalps",
      "Isochores",
    ]);
  });

  test("labels name a line's value in the shown units", () => {
    expect(lineLabel("T", 300, "si")).toBe("300 K");
    expect(lineLabel("T", 300, "engineering")).toBe("26.85 °C");
    expect(lineLabel("P", 1e5, "engineering")).toBe("1 bar");
    expect(lineLabel("Dmass", 10, "si")).toBe("10 kg/m³");
    expect(lineLabel("Smass", 2000, "engineering")).toBe("2 kJ/(kg·K)");
    expect(lineLabel("Q", 0.5, "si")).toBe("q = 0.5");
  });
});

describe("scales", () => {
  test("pad a range by 4 % of its span, or of its decades on a log axis", () => {
    expect(padded([0, 100], false)).toEqual([-4, 104]);
    const [a, b] = padded([1, 1e4], true);
    expect([Math.log10(a), Math.log10(b)].map((x) => Number(x.toFixed(9)))).toEqual([-0.16, 4.16]);
  });

  test("span the plot in the shown units, and the readout maps a pixel back to SI inputs", () => {
    const { x, y, plot } = baseScales(ph, data, 800, 500, "engineering");
    expect(plot).toEqual({ left: 72, top: 16, right: 784, bottom: 452 });
    expect(x.domain()).toEqual(padded([0, 3000], false));
    expect(y.range()).toEqual([452, 16]);
    expect(y.domain().map((d) => Number(Math.log10(d).toFixed(9)))).toEqual([-2.2, 3.2]);
    const [k1, v1, k2, v2] = readoutInputs(ph, x, y, x(1500), y(1), "engineering");
    expect([k1, k2]).toEqual(["Hmass", "P"]);
    expect(v1).toBeCloseTo(1.5e6, 6);
    expect(v2).toBeCloseTo(1e5, 6);
    const si = baseScales(ph, data, 800, 500, "si");
    expect(
      readoutInputs(ph, si.x, si.y, si.x(2e6), si.y(1e6), "si").map((n) => (typeof n === "number" ? Math.round(n) : n)),
    ).toEqual(["Hmass", 2e6, "P", 1e6]);
  });
});

describe("the drawing model", () => {
  test("draws the dome, each shown line in its family's style, the critical point, ticks and titles", () => {
    const { x, y, plot } = baseScales(ph, data, 800, 500, "si");
    const model = buildModel(ph, data, { x, y, plot, system: "si", hidden: new Set(), colour: colours });
    expect(model.strokes.length).toBe(4);
    expect(model.strokes[0]).toMatchObject({ colour: "rgb(--fg)", width: 2, dash: [] });
    expect(model.strokes[0]?.points.length).toBe(5);
    expect(model.strokes[1]).toMatchObject({ colour: "rgb(--iso-1)", dash: [], width: 1.5 });
    expect(model.strokes[3]).toMatchObject({ colour: "rgb(--fg-muted)", dash: [2, 3], width: 1 });
    expect(model.strokes[1]?.points[0]).toEqual([x(2700e3), y(1e3)]);
    expect(model.dots).toEqual([{ x: x(2000e3), y: y(2.2e7), r: 4, colour: "rgb(--fg)" }]);
    expect(model.labels.map((l) => l.text)).toEqual(["400 K", "6000 J/(kg·K)", "q = 0.5"]);
    expect(model.titles).toEqual({ x: "h / J/kg", y: "p / Pa" });
    expect(model.xTicks.length).toBeGreaterThan(3);
    expect(model.yTicks.map((t) => t.text)).toEqual(["1000", "10⁴", "10⁵", "10⁶", "10⁷", "10⁸"]);
    expect(model.xTicks.every((t) => t.at >= plot.left && t.at <= plot.right)).toBe(true);
    const hidden = buildModel(ph, data, { x, y, plot, system: "si", hidden: new Set(["T", "Q"]), colour: colours });
    expect(hidden.strokes.length).toBe(2);
    expect(hidden.labels.map((l) => l.text)).toEqual(["6000 J/(kg·K)"]);
  });

  test("labels a line at a free end, none overlapping: a quality line at its cool end or not at all", () => {
    const plot = { left: 0, top: 0, right: 400, bottom: 300 };
    const [x, y] = [scaleLinear().domain([0, 400]).range([0, 400]), scaleLinear().domain([0, 300]).range([0, 300])];
    const at = (px: number, py: number): Vertex => ({ x: px, y: py, inputs: ["T", 1, "P", 1] });
    const flat: Diagram = { ...ph, x: { key: "Hmass", log: false }, y: { key: "Smass", log: false } } as Diagram;
    const line = (held: "T" | "Q", value: number, ...points: Vertex[]) => ({
      held,
      value,
      segments: [{ twoPhase: false, vertices: points }],
    });
    const crowded: DiagramData = {
      dome: { liquid: [], vapour: [], critical: null },
      lines: [
        line("T", 1, at(100, 200), at(100, 100)),
        line("T", 2, at(100, 202), at(100, 102)),
        line("T", 3, at(101, 203), at(101, 103)),
        line("Q", 0.5, at(50, 250), at(60, 150)),
        line("Q", 0.6, at(52, 251), at(300, 150)),
        line("T", 4, at(390, 50), at(395, 60)),
        line("T", 5, at(200, 2), at(210, 5)),
        line("T", 6, at(-50, -50), at(-60, -60)),
      ],
      x: [0, 400],
      y: [0, 300],
    };
    const m = buildModel(flat, crowded, { x, y, plot, system: "si", hidden: new Set(), colour: colours });
    expect(m.labels).toEqual([
      { x: 104, y: 96, text: "1 K", anchor: "start" },
      { x: 104, y: 198, text: "2 K", anchor: "start" },
      { x: 54, y: 246, text: "q = 0.5", anchor: "start" },
      { x: 391, y: 56, text: "4 K", anchor: "end" },
      { x: 214, y: 19, text: "5 K", anchor: "start" },
    ]);
  });

  test("thins ticks so neighbours keep their labels' width apart", () => {
    const trho = diagram("trho") as Diagram;
    const wide: DiagramData = { ...data, x: [1e-7, 1e3], y: [100, 600] };
    const { x, y, plot } = baseScales(trho, wide, 360, 400, "si");
    const m = buildModel(trho, wide, { x, y, plot, system: "si", hidden: new Set(), colour: colours });
    expect(m.xTicks.length).toBeGreaterThan(2);
    m.xTicks.forEach((t, i) => {
      const next = m.xTicks[i + 1];
      if (next) expect(next.at - t.at, `${t.text} to ${next.text}`).toBeGreaterThanOrEqual(t.text.length * 6.5 + 12);
    });
    m.yTicks.forEach((t, i) => {
      const next = m.yTicks[i + 1];
      if (next) expect(Math.abs(next.at - t.at)).toBeGreaterThanOrEqual(18);
    });
    const all = buildModel(trho, wide, {
      ...baseScales(trho, wide, 2000, 400, "si"),
      system: "si",
      hidden: new Set(),
      colour: colours,
    });
    expect(all.xTicks.map((t) => t.text)).toEqual([
      "10⁻⁷",
      "10⁻⁶",
      "10⁻⁵",
      "10⁻⁴",
      "0.001",
      "0.01",
      "0.1",
      "1",
      "10",
      "100",
      "1000",
    ]);
  });

  test("renders as one SVG: background, frame, clipped lines, ticks, titles", () => {
    const { x, y, plot } = baseScales(ph, data, 800, 500, "si");
    const model = buildModel(ph, data, { x, y, plot, system: "si", hidden: new Set(), colour: colours });
    const svg = renderSvg(model, {
      width: 800,
      height: 500,
      background: "#fff",
      ink: "#111",
      muted: "#555",
      font: "Inter",
    });
    expect(
      svg.startsWith('<svg xmlns="http://www.w3.org/2000/svg" width="800" height="500" viewBox="0 0 800 500"'),
    ).toBe(true);
    expect(svg.match(/<path /g)?.length).toBe(4);
    expect(svg).toContain('stroke-dasharray="2 3"');
    expect(svg).toContain(">h / J/kg</text>");
    expect(svg).toContain('<rect width="800" height="500" fill="#fff"/>');
    expect(svg).toContain('clip-path="url(#plot)"');
    expect(svg.endsWith("</svg>")).toBe(true);
    expect(
      renderSvg(
        { ...model, labels: [{ x: 1, y: 2, text: "a < b & c", anchor: "end" }] },
        { width: 1, height: 1, background: "", ink: "", muted: "", font: "" },
      ),
    ).toContain("a &lt; b &amp; c");
  });

  test("paths round to a tenth of a pixel", () => {
    expect(
      pathOf([
        [0, 0],
        [10.04, 20.06],
        [3.25, -1],
      ]),
    ).toBe("M0,0L10,20.1L3.3,-1");
    expect(pathOf([])).toBe("");
  });
});

describe("log ticks", () => {
  test("are decades, plain up to a thousand and down to a thousandth, powers of ten past that", () => {
    expect([1e-5, 1e-3, 0.1, 1, 1000, 1e4, 1e12].map(decadeText)).toEqual([
      "10⁻⁵",
      "0.001",
      "0.1",
      "1",
      "1000",
      "10⁴",
      "10¹²",
    ]);
  });
});

describe("the CSV", () => {
  test("lists every vertex in SI, the dome's and the critical point's first", () => {
    const csv = toCsv(ph, data).trimEnd().split("\n");
    expect(csv[0]).toBe("line,value,segment,vertex,Hmass,P");
    expect(csv.length).toBe(1 + 2 + 2 + 1 + 2 + 2 + 2);
    expect(csv[1]).toBe("liquid,,0,0,100000,1000");
    expect(csv[5]).toBe("critical,,0,0,2000000,22000000");
    expect(csv[6]).toBe("T,400,0,0,2700000,1000");
    expect(csv.at(-1)).toBe("Q,0.5,0,1,1400000,100000");
  });
});

describe("label boxes", () => {
  const box = { x0: 0, x1: 10, y0: 0, y1: 10 };

  test("overlap only where they share area, not where they touch", () => {
    expect(overlaps(box, { x0: 10, x1: 20, y0: 0, y1: 10 })).toBe(false);
    expect(overlaps(box, { x0: -10, x1: 0, y0: 0, y1: 10 })).toBe(false);
    expect(overlaps(box, { x0: 0, x1: 10, y0: 10, y1: 20 })).toBe(false);
    expect(overlaps(box, { x0: 0, x1: 10, y0: -10, y1: 0 })).toBe(false);
    expect(overlaps(box, { x0: 9.9, x1: 20, y0: 0, y1: 10 })).toBe(true);
    expect(overlaps(box, { x0: -10, x1: 0.1, y0: 0, y1: 10 })).toBe(true);
    expect(overlaps(box, { x0: 0, x1: 10, y0: 9.9, y1: 20 })).toBe(true);
    expect(overlaps(box, { x0: 0, x1: 10, y0: -10, y1: 0.1 })).toBe(true);
  });

  test("sit right of and above their point, until that leaves the plot", () => {
    const plot = { left: 0, top: 0, right: 100, bottom: 100 };
    const placed: Box[] = [];
    // "ab" is 13 px wide: at x = 83 it ends on the plot's right side, still inside.
    expect(place("ab", [[83, 50]], plot, placed)).toEqual({ x: 87, y: 46, text: "ab", anchor: "start" });
    expect(placed).toEqual([{ x0: 87, x1: 100, y0: 36, y1: 46 }]);
    expect(place("ab", [[83.5, 80]], plot, placed)).toEqual({ x: 79.5, y: 76, text: "ab", anchor: "end" });
    expect(placed[1]).toEqual({ x0: 66.5, x1: 79.5, y0: 66, y1: 76 });
    // At y = 14 the label's top is the plot's top; any higher and it goes below its point.
    expect(place("ab", [[20, 14]], plot, placed)?.y).toBe(10);
    expect(place("ab", [[50, 13.5]], plot, placed)?.y).toBe(27.5);
    expect(place("ab", [[20, 14]], plot, placed)).toBeNull();
  });
});

describe("the model at its edges", () => {
  const plot = { left: 0, top: 0, right: 400, bottom: 300 };
  const flat = { ...ph, x: { key: "Hmass", log: false }, y: { key: "Smass", log: false } } as Diagram;
  const at = (px: number, py: number): Vertex => ({ x: px, y: py, inputs: ["T", 1, "P", 1] });
  const line = (value: number, ...points: Vertex[]) => ({
    held: "T" as const,
    value,
    segments: [{ twoPhase: false, vertices: points }],
  });
  const model = (lines: DiagramData["lines"], xd: [number, number] = [0, 400], yd: [number, number] = [0, 300]) =>
    buildModel(
      flat,
      { dome: { liquid: [], vapour: [], critical: null }, lines, x: [0, 1], y: [0, 1] },
      {
        x: scaleLinear().domain(xd).range([xd[0], xd[1]]),
        y: scaleLinear().domain(yd).range([yd[0], yd[1]]),
        plot,
        system: "si",
        hidden: new Set(),
        colour: colours,
      },
    );

  test("counts a point on the plot's edge as inside, and one past it as outside", () => {
    const labelled = (...points: Vertex[]) => model([line(1, ...points)]).labels.length;
    expect(labelled(at(-5, 150), at(0, 150))).toBe(1);
    expect(labelled(at(405, 150), at(400, 150))).toBe(1);
    expect(labelled(at(200, -5), at(200, 0))).toBe(1);
    expect(labelled(at(200, 305), at(200, 300))).toBe(1);
    for (const outside of [at(-0.1, 150), at(400.1, 150), at(200, -0.1), at(200, 300.1)]) {
      expect(labelled(outside), JSON.stringify(outside)).toBe(0);
    }
  });

  test("labels a line at its last point in the plot", () => {
    const m = model([line(1, at(10, 200), at(20, 150), at(30, 100))]);
    expect(m.labels).toEqual([{ x: 34, y: 96, text: "1 K", anchor: "start" }]);
  });

  test("keeps ticks inside the plot, its edges included, and a label's width apart", () => {
    const texts = (ticks: { text: string }[]) => ticks.map((t) => t.text);
    expect(texts(model([], [0, 400]).xTicks)).toEqual(["0", "50", "100", "150", "200", "250", "300", "350", "400"]);
    // Domains of powers of two keep d3's interpolation exact: a tick on the plot's edge lands on it, not 1e-14 off.
    expect(texts(model([], [-128, 384]).xTicks)).toEqual(["0", "100", "200", "300"]);
    expect(texts(model([], [0, 512]).xTicks)).toEqual(["0", "100", "200", "300", "400"]);
    expect(texts(model([], [0, 300], [0, 300]).yTicks)).toEqual(["0", "50", "100", "150", "200", "250", "300"]);
    // Ticks 20 apart at 0.925 px each: 18.5 px, exactly the gap "0" needs (6.5 + 12) but less than "20" needs.
    const tight = buildModel(
      flat,
      { dome: { liquid: [], vapour: [], critical: null }, lines: [], x: [0, 1], y: [0, 1] },
      {
        x: scaleLinear().domain([0, 100]).range([0, 92.5]),
        y: scaleLinear().domain([0, 1]).range([300, 0]),
        plot: { left: 0, top: 0, right: 92.5, bottom: 300 },
        system: "si",
        hidden: new Set(),
        colour: colours,
      },
    );
    expect(texts(tight.xTicks)).toEqual(["0", "20", "60", "100"]);
  });
});

describe("the dome and the files", () => {
  test("runs the dome up the liquid side, through the critical point, down the vapour side", () => {
    const { x, y, plot } = baseScales(ph, data, 800, 500, "si");
    const px = (v: Vertex) => [x(v.x), y(v.y)];
    const m = buildModel(ph, data, { x, y, plot, system: "si", hidden: new Set(), colour: colours });
    const { liquid, vapour, critical } = data.dome;
    expect(m.strokes[0]?.points).toEqual([...liquid, critical as Vertex, ...[...vapour].reverse()].map(px));
    const none = { ...data, dome: { ...data.dome, critical: null } };
    const n = buildModel(ph, none, { x, y, plot, system: "si", hidden: new Set(), colour: colours });
    expect(n.strokes[0]?.points).toEqual([...liquid, ...[...vapour].reverse()].map(px));
    expect(n.dots).toEqual([]);
    const csv = toCsv(ph, none).trimEnd().split("\n");
    expect(csv.length).toBe(1 + 2 + 2 + 6);
    expect(csv[3]).toBe("vapour,,0,0,2500000,1000");
    expect(csv[5]).toBe("T,400,0,0,2700000,1000");
    expect(padded([10, 110], false)).toEqual([6, 114]);
  });

  test("writes an SVG exactly", () => {
    const tiny: Model = {
      plot: { left: 10, top: 5, right: 90, bottom: 45 },
      strokes: [
        {
          points: [
            [10, 45],
            [50.04, 5.06],
          ],
          colour: "red",
          dash: [],
          width: 2,
        },
        { points: [[20, 20]], colour: "blue", dash: [2, 3], width: 1 },
      ],
      dots: [{ x: 50.04, y: 5.06, r: 4, colour: "red" }],
      labels: [{ x: 60, y: 30, text: "a>b", anchor: "start" }],
      xTicks: [{ at: 10, text: "0" }],
      yTicks: [{ at: 45, text: "1" }],
      titles: { x: "x / m", y: "y / s" },
    };
    expect(
      renderSvg(tiny, { width: 100, height: 60, background: "#fff", ink: "#000", muted: "#777", font: "Inter" }),
    ).toBe(
      [
        '<svg xmlns="http://www.w3.org/2000/svg" width="100" height="60" viewBox="0 0 100 60" font-family="Inter" font-size="12">',
        '<defs><clipPath id="plot"><rect x="10" y="5" width="80" height="40"/></clipPath></defs>',
        '<rect width="100" height="60" fill="#fff"/>',
        '<rect x="10" y="5" width="80" height="40" fill="none" stroke="#777"/>',
        '<g clip-path="url(#plot)"><path d="M10,45L50,5.1" fill="none" stroke="red" stroke-width="2"/>' +
          '<path d="M20,20" fill="none" stroke="blue" stroke-width="1" stroke-dasharray="2 3"/>' +
          '<circle cx="50" cy="5.1" r="4" fill="red"/>' +
          '<text x="60" y="30" text-anchor="start" fill="#777">a&gt;b</text></g>',
        '<text x="10" y="63" text-anchor="middle" fill="#777">0</text>',
        '<text x="2" y="49" text-anchor="end" fill="#777">1</text>',
        '<text x="50" y="52" text-anchor="middle" fill="#000">x / m</text>',
        '<text transform="translate(16 25) rotate(-90)" text-anchor="middle" fill="#000">y / s</text>',
        "</svg>",
      ].join("\n"),
    );
  });
});

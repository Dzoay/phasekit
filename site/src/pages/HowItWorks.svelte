<script lang="ts">
  // How a property is calculated (PLAN.md M21.10, the user's request, 2026-10-17): the property pipeline of
  // phasekit-core in four static figures, traced from crates/phasekit-core/src/flash.rs, flash/phase.rs, fluid.rs,
  // state.rs and batch.rs. Each figure is data drawn as inline SVG by the `figure` snippet and themed by the page's
  // tokens: the Helmholtz model in the accent, what loads on first use dashed in the first isoline colour, refusals
  // dotted. Coordinates are the figure's viewBox units; a text that starts with ` is set as code.

  type Point = [number, number];
  type Kind = "plain" | "model" | "lazy" | "refused";

  interface Box {
    at: [number, number, number, number];
    title: string;
    lines?: string[];
    kind?: Kind;
    center?: boolean;
  }

  interface Edge {
    path: Point[];
    kind?: Kind;
    /** Arrowheads at both ends. */
    both?: boolean;
    /** No arrowhead: a trunk that edges branch from. */
    bare?: boolean;
  }

  interface Text {
    at: Point;
    text: string;
    middle?: boolean;
    /** Its classes; a label on a halo of the frame by default. */
    kind?: string;
  }

  interface Figure {
    id: string;
    view: Point;
    /** The width below which the figure scrolls in its frame rather than shrink. */
    min: number;
    claim: string;
    frames?: { at: [number, number, number, number]; kind: "group" | "loop" }[];
    chips?: { at: Point; text: string }[];
    boxes: Box[];
    edges: Edge[];
    texts: Text[];
  }

  const KINDS: Kind[] = ["plain", "model", "lazy", "refused"];
  const SOURCE = "https://github.com/Dzoay/phasekit/blob/main/crates/phasekit-core/src/";
  const FILES = ["flash.rs", "flash/phase.rs", "fluid.rs", "state.rs", "batch.rs"];

  const down = (x: number, y0: number, y1: number): Point[] => [
    [x, y0],
    [x, y1],
  ];
  const across = (y: number, x0: number, x1: number): Point[] => [
    [x0, y],
    [x1, y],
  ];

  const CALL: Figure = {
    id: "call",
    view: [840, 990],
    min: 680,
    claim:
      "One property call: the caller gets a Fluid from the Registry, Fluid::flash normalises the input, the pure-fluid " +
      "flash runs the phase rule and solves for temperature and density, evaluating the Helmholtz model at every " +
      "iterate, the acceptance gate checks the result, and the outputs are read from the State.",
    frames: [{ at: [14, 336, 812, 300], kind: "group" }],
    boxes: [
      {
        at: [24, 20, 380, 68],
        title: "Caller",
        lines: ['`registry.get("Water")?.flash(input, &opts)', "or PropsSI strings via compat · C ABI · WASM · batch"],
      },
      {
        at: [24, 124, 380, 68],
        title: '`Registry::get("Water")',
        lines: ["name, alias or CAS, ASCII case-insensitive", "after first use: one atomic load, no allocation"],
      },
      {
        at: [520, 20, 296, 68],
        title: "phasekit-data blob or a Pack",
        lines: ["one checksummed LE blob per fluid, built by", "xtask datagen from CoolProp 8.0.0's JSON"],
      },
      {
        at: [520, 124, 296, 68],
        title: "Slot · OnceLock",
        lines: ["first get only: validate the blob,", "apply corrections, compile the EOS"],
        kind: "lazy",
      },
      {
        at: [24, 228, 380, 68],
        title: "Fluid::flash: normalise once",
        lines: ["capability check · basis → molar · gauge → native", "`Input → NativeInput"],
      },
      {
        at: [44, 366, 340, 51],
        title: "domain check · dispatch on Pair",
        lines: ["limits under Enforce; one arm per pair (19)"],
      },
      { at: [44, 452, 340, 51], title: "phase rule", lines: ["hint → T ≥ Tc → saturation at T → ρ vs ρ′, ρ″"] },
      { at: [44, 548, 340, 51], title: "solve for (T, ρ)", lines: ["e.g. PT: safeguarded density Newton on p(ρ)"] },
      {
        at: [520, 444, 296, 68],
        title: "saturation curve · LazyLock",
        lines: ["superancillary, loaded on first use", "Exact: used · Guess: seeds a VLE polish"],
        kind: "lazy",
      },
      {
        at: [520, 531, 296, 85],
        title: "HelmholtzModel: α⁰ + α^r",
        lines: [
          "at (T, ρ) to order 2 → Bundle a00 … a02",
          "one exp per term, no per-term derivatives",
          "same code on f64, Jet4 and SIMD lanes",
        ],
        kind: "model",
      },
      {
        at: [24, 666, 380, 68],
        title: "acceptance gate",
        lines: ["the state reproduces its inputs", "single phase: ∂p/∂ρ > 0 and cv > 0"],
      },
      {
        at: [520, 666, 296, 68],
        title: "typed Error, never a wrong value",
        lines: ["`NoConvergence · Domain · Ambiguous", "`Unsupported · Undefined · Load"],
        kind: "refused",
      },
      {
        at: [24, 770, 380, 68],
        title: "State",
        lines: ["T, ρ and a Bundle per phase point (one or two), q", "SolvePath · gauge stamp · Copy, at most 256 B"],
      },
      {
        at: [24, 884, 252, 85],
        title: "relations on the Bundle",
        lines: [
          "p h s u g cv cp w Z from six numbers",
          "first partials by Jacobian ratio",
          "no model call, no allocation",
        ],
      },
      {
        at: [294, 884, 252, 85],
        title: "derivs to order 3–4",
        lines: ["second partials, κ_T, β, Γ", "Cp0, residual h s g, α outputs", "one more EOS call at the state"],
      },
      {
        at: [564, 884, 252, 85],
        title: "transport · LazyLock",
        lines: ["η, λ: staged sums or ECS", "σ(T) from its fit", "first call loads set and ECS refs"],
        kind: "lazy",
      },
    ],
    edges: [
      { path: down(214, 88, 124) },
      { path: down(668, 88, 124), kind: "lazy" },
      { path: across(158, 520, 404), kind: "lazy" },
      { path: down(214, 192, 228) },
      { path: down(214, 296, 366) },
      { path: down(214, 417, 452) },
      { path: down(214, 503, 548) },
      { path: across(478, 520, 384), kind: "lazy" },
      { path: across(562, 384, 520), kind: "model" },
      { path: across(586, 520, 384), kind: "model" },
      { path: down(214, 599, 666) },
      { path: across(700, 404, 520), kind: "refused" },
      { path: down(214, 734, 770) },
      { path: down(214, 838, 862), bare: true },
      { path: across(862, 150, 690), bare: true },
      { path: down(150, 862, 884) },
      { path: down(420, 862, 884) },
      { path: down(690, 862, 884) },
    ],
    texts: [
      { at: [224, 110], text: "name" },
      { at: [678, 110], text: "decoded once" },
      { at: [462, 151], text: "Fluid", middle: true },
      { at: [224, 214], text: "`&Fluid = Arc<dyn ThermoModel> + Gauge" },
      { at: [224, 322], text: "`NativeInput, FlashOptions" },
      { at: [28, 354], text: "PUREFLUID · THERMOMODEL::FLASH", kind: "label caps" },
      { at: [452, 471], text: "ρ′, ρ″, p_sat", middle: true },
      { at: [452, 555], text: "T, ρ", middle: true, kind: "label accent" },
      { at: [452, 602], text: "Bundle", middle: true, kind: "label accent" },
      { at: [452, 617], text: "every iterate", middle: true, kind: "label accent" },
      { at: [224, 654], text: "candidate (T, ρ), SolvePath" },
      { at: [462, 693], text: "refused", middle: true },
      { at: [224, 756], text: "accepted" },
      { at: [224, 855], text: "`Fluid::prop(&state, Prop)" },
    ],
  };

  // Rows of the pairs figure, 64 apart from 70: the pairs, top to bottom; each outer search at its row; the pairs that go
  // straight to a primitive; the edges from a search to the primitive its iterates are.
  const PAIRS = ["DT", "DH DS DU DP", "HT ST TU", "PT", "HP PS PU", "HS SU", "QT PQ", "QS HQ DQ"];
  const SEARCHES: [number, string, string][] = [
    [134, "isochore", "TOMS 748 in T at fixed ρ"],
    [198, "isotherm", "TOMS 748 in ln ρ at fixed T"],
    [326, "isobar", "Newton, else TOMS 748 in T"],
    [390, "isentrope", "TOMS 748 in ln p at fixed s"],
    [518, "along saturation", "TOMS 748 in T on the curves"],
  ];
  const DIRECT: [number, string][] = [
    [70, "(T, ρ) given: no search"],
    [262, "one density solve"],
    [454, "Q given: lever rule"],
  ];
  const ITERATES: [number, string][] = [
    [134, "DT per iterate"],
    [198, "DT per iterate"],
    [326, "ρ(T, p) per iterate"],
    [518, "QT per iterate"],
  ];

  const PAIRS_FIGURE: Figure = {
    id: "pairs",
    view: [940, 566],
    min: 760,
    claim:
      "The 19 input pairs: DT, PT, and QT with PQ go straight to their primitive; the other pairs run an outer search " +
      "along an isochore, isotherm, isobar, isentrope or the saturation curve, whose iterates are DT, PT or QT states; " +
      "every primitive evaluates the Helmholtz model.",
    chips: PAIRS.map((text, i) => ({ at: [16, 54 + 64 * i], text })),
    boxes: [
      ...SEARCHES.map(([y, title, line]): Box => ({ at: [184, y - 24, 220, 48], title, lines: [line] })),
      {
        at: [532, 46, 210, 176],
        title: "DT · direct",
        lines: ["one bundle at (T, ρ)", "phase rule, no iteration", "`Strategy::Direct"],
        center: true,
      },
      {
        at: [532, 238, 210, 112],
        title: "density Newton",
        lines: ["ρ at (T, p), safeguarded,", "bracketed on the phase branch", "`Strategy::DensityNewton"],
        center: true,
      },
      {
        at: [532, 430, 210, 112],
        title: "saturation at T or p",
        lines: ["superancillary when Exact,", "else a VLE solve or polish", "`Superancillary · Vle"],
        center: true,
      },
      { at: [806, 46, 118, 496], title: "", kind: "model" },
    ],
    edges: [
      ...SEARCHES.map(([y]) => ({ path: across(y, 136, 184) })),
      ...DIRECT.map(([y]) => ({ path: across(y, 136, 532) })),
      ...ITERATES.map(([y]) => ({ path: across(y, 404, 532) })),
      { path: down(294, 366, 350) },
      ...[134, 294, 486].map((y): Edge => ({ path: across(y, 742, 806), kind: "model", both: true })),
    ],
    texts: [
      { at: [16, 26], text: "INPUT PAIR", kind: "label caps" },
      { at: [184, 26], text: "OUTER SEARCH", kind: "label caps" },
      { at: [532, 26], text: "PRIMITIVE", kind: "label caps" },
      { at: [806, 26], text: "MODEL", kind: "label caps" },
      ...DIRECT.map(([y, text]): Text => ({ at: [294, y - 7], text, middle: true })),
      ...ITERATES.map(([y, text]): Text => ({ at: [468, y - 7], text, middle: true })),
      { at: [304, 361.5], text: "PS flash per iterate", kind: "label small" },
      { at: [865, 250], text: "Helmholtz", middle: true, kind: "title model" },
      { at: [865, 268], text: "model", middle: true, kind: "title model" },
      { at: [865, 304], text: "α⁰ + α^r", middle: true, kind: "sub" },
      { at: [865, 322], text: "at (T, ρ)", middle: true, kind: "sub" },
      { at: [865, 340], text: "→ Bundle", middle: true, kind: "sub" },
    ],
  };

  const PHASE: Figure = {
    id: "phase",
    view: [940, 346],
    min: 760,
    claim:
      "The phase rule for a temperature and density: a phase hint is taken as given; at or above the critical " +
      "temperature the phase follows from pressure and density against the critical point; below it, saturation at T " +
      "comes from an exact superancillary, a VLE polish of a guess curve, or a generic VLE, and the density is compared " +
      "with the saturated densities.",
    boxes: [
      { at: [16, 40, 170, 60], title: "phase hint given?", lines: ["FlashOptions, per call"] },
      { at: [226, 40, 180, 60], title: "T ≥ Tc ?", lines: ["Tc: the model's own"] },
      {
        at: [446, 34, 220, 112],
        title: "saturation at T",
        lines: [
          "Exact curve: superancillary",
          "Guess curve: VLE polish",
          "no curve over T: generic VLE",
          "never a fit outside its range",
        ],
      },
      { at: [706, 40, 218, 60], title: "compare ρ with ρ′ and ρ″", lines: ["the saturated densities at T"] },
      { at: [16, 190, 170, 68], title: "that phase", lines: ["taken as given;", "no stability check"] },
      { at: [226, 190, 180, 68], title: "single phase", lines: ["by p and ρ against", "the critical point"] },
      { at: [746, 138, 178, 52], title: "ρ ≥ ρ′: liquid", lines: ["above pc: supercritical"] },
      { at: [746, 196, 178, 40], title: "ρ ≤ ρ″: gas", center: true },
      {
        at: [746, 250, 178, 84],
        title: "between: two phases",
        lines: ["q by lever rule on 1/ρ", "each side's bundle from", "the EOS at ρ′ or ρ″"],
      },
    ],
    edges: [
      { path: across(70, 186, 226) },
      { path: across(70, 406, 446) },
      { path: across(70, 666, 706) },
      { path: down(101, 100, 190) },
      { path: down(316, 100, 190) },
      { path: down(724, 100, 292), bare: true },
      ...[158, 216, 292].map((y) => ({ path: across(y, 724, 746) })),
    ],
    texts: [
      { at: [206, 63], text: "no", middle: true },
      { at: [426, 63], text: "no", middle: true },
      { at: [109, 150], text: "yes" },
      { at: [324, 150], text: "yes" },
      { at: [470, 222], text: "a single phase the rule chose then" },
      { at: [470, 238], text: "passes the stability check:" },
      { at: [470, 254], text: "∂p/∂ρ > 0 and cv > 0" },
    ],
  };

  const BATCH: Figure = {
    id: "batch",
    view: [940, 356],
    min: 760,
    claim:
      "Two batch policies. Reference: each point runs Input::new, then the flash with the Helmholtz model inside its " +
      "loop, then the outputs. Simd, for DT, QT and PQ: the model is evaluated for 64 points at once in lanes, then each " +
      "point's flash runs on its own bundle.",
    frames: [
      { at: [16, 34, 908, 152], kind: "loop" },
      { at: [496, 236, 428, 112], kind: "loop" },
    ],
    boxes: [
      { at: [40, 64, 170, 50], title: "Input::new", lines: ["`(pair, x[i], y[i])"] },
      { at: [290, 64, 260, 50], title: "Fluid::flash", lines: ["phase rule · solve for (T, ρ)"] },
      { at: [290, 136, 260, 42], title: "α⁰ + α^r → Bundle", kind: "model", center: true },
      { at: [630, 64, 280, 50], title: "outputs", lines: ["`out[i·M + k] and a Status byte"] },
      { at: [16, 262, 170, 50], title: "64 points", lines: ["one group on the stack"] },
      { at: [226, 262, 240, 50], title: "α⁰ + α^r in lanes", lines: ["QT, PQ: saturation sums too"], kind: "model" },
      { at: [512, 262, 200, 50], title: "flash on its bundle", lines: ["phase rule, checks, sides"] },
      { at: [742, 262, 170, 50], title: "outputs", lines: ["same bits as Reference"] },
    ],
    edges: [
      { path: across(89, 210, 290) },
      { path: down(420, 114, 136), kind: "model", both: true },
      { path: across(89, 550, 630) },
      { path: across(287, 186, 226), kind: "model" },
      { path: across(287, 466, 512), kind: "model" },
      { path: across(287, 712, 742) },
    ],
    texts: [
      { at: [16, 22], text: "EXECPOLICY::REFERENCE · EVERY PAIR", kind: "label caps" },
      { at: [28, 52], text: "for each point i", kind: "label small" },
      { at: [430, 129], text: "per iterate", kind: "label small accent" },
      { at: [590, 82], text: "State", middle: true },
      { at: [16, 224], text: "EXECPOLICY::SIMD · DT, QT, PQ", kind: "label caps" },
      { at: [508, 254], text: "for each point i", kind: "label small" },
    ],
  };

  /** The y of a box's title; its lines follow 18 then 17 apart. A centred box centres the block. */
  function titleY({ at: [, y, , h], lines = [], center }: Box): number {
    return center ? y + (h - 16 - 17 * lines.length) / 2 + 13 : y + 22;
  }
</script>

{#snippet text(t: Text)}
  {@const code = t.text.startsWith("`")}
  <text
    x={t.at[0]}
    y={t.at[1]}
    class={[t.kind ?? "label", code && "code"]}
    text-anchor={t.middle ? "middle" : undefined}>{code ? t.text.slice(1) : t.text}</text
  >
{/snippet}

{#snippet figure(f: Figure)}
  <div class="frame" data-testid="figure-frame">
    <svg viewBox="0 0 {f.view[0]} {f.view[1]}" style="min-width: {f.min}px" role="img" aria-label={f.claim}>
      <defs>
        {#each KINDS as kind (kind)}
          <marker
            id="{f.id}-{kind}"
            viewBox="0 0 10 10"
            refX="9"
            refY="5"
            markerWidth="7"
            markerHeight="7"
            orient="auto"
          >
            <path d="M0 1 L10 5 L0 9 z" class={["tip", kind]} />
          </marker>
          <marker
            id="{f.id}-{kind}-start"
            viewBox="0 0 10 10"
            refX="1"
            refY="5"
            markerWidth="7"
            markerHeight="7"
            orient="auto"
          >
            <path d="M10 1 L0 5 L10 9 z" class={["tip", kind]} />
          </marker>
        {/each}
      </defs>
      {#each f.frames ?? [] as { at: [x, y, w, h], kind }, i (i)}
        <rect {x} {y} width={w} height={h} rx="5" class={kind} />
      {/each}
      {#each f.chips ?? [] as { at: [x, y], text }, i (i)}
        <rect {x} {y} width="120" height="32" rx="16" class="chip" />
        <text x={x + 60} y={y + 20.5} class="chip-label" text-anchor="middle">{text}</text>
      {/each}
      {#each f.boxes as b, i (i)}
        {@const [x, y, w, h] = b.at}
        {@const top = titleY(b)}
        {@const left = b.center ? x + w / 2 : x + 14}
        <rect
          {x}
          {y}
          width={w}
          height={h}
          rx="3"
          class={["box", b.kind]}
          data-testid={b.kind === "model" ? "model" : undefined}
        />
        {@render text({ at: [left, top], text: b.title, middle: b.center, kind: `title ${b.kind ?? ""}` })}
        {#each b.lines ?? [] as line, j (j)}
          {@render text({ at: [left, top + 18 + 17 * j], text: line, middle: b.center, kind: "sub" })}
        {/each}
      {/each}
      {#each f.edges as e, i (i)}
        {@const kind = e.kind ?? "plain"}
        <path
          d={"M" + e.path.map(([x, y]) => `${x} ${y}`).join(" L")}
          class={["edge", kind]}
          marker-end={e.bare ? undefined : `url(#${f.id}-${kind})`}
          marker-start={e.both ? `url(#${f.id}-${kind}-start)` : undefined}
        />
      {/each}
      {#each f.texts as t, i (i)}
        {@render text(t)}
      {/each}
    </svg>
  </div>
{/snippet}

<article class="how">
  <header class="card part">
    <h1>How a property is calculated</h1>
    <p class="lede">
      Every property call does three things. It finds the state's temperature and density: one point, or two inside the
      vapour dome. It evaluates the Helmholtz energy at each point, which gives six numbers, the <code>Bundle</code>.
      Then p, h, s, c<sub>p</sub>, w and the first partial derivatives are arithmetic on those six numbers. The 19 input
      pairs differ only in how they search for (T, ρ).
    </p>
    <ul class="legend" aria-label="Legend">
      <li>
        <svg viewBox="0 0 34 12" aria-hidden="true"><line x1="1" y1="6" x2="33" y2="6" class="edge plain" /></svg>
        every call
      </li>
      <li>
        <svg viewBox="0 0 34 12" aria-hidden="true"><line x1="1" y1="6" x2="33" y2="6" class="edge lazy" /></svg>
        once per fluid, on first use
      </li>
      <li>
        <svg viewBox="0 0 34 12" aria-hidden="true"
          ><rect x="1" y="1" width="32" height="10" rx="2" class="box model" /></svg
        >
        Helmholtz model evaluation
      </li>
      <li>
        <svg viewBox="0 0 34 12" aria-hidden="true"><line x1="1" y1="6" x2="33" y2="6" class="edge refused" /></svg>
        refusal, a typed error
      </li>
    </ul>
  </header>

  <section class="card part">
    <h2>One call, end to end</h2>
    <p>
      A PT call for water, from the caller down to the number. The <a href="#/calculator">calculator</a> runs this path in
      a Web Worker through the browser package. The dashed parts run once per fluid and are a single atomic load afterwards,
      so a workload that never asks for saturation or transport never decodes their data.
    </p>
    <figure>
      {@render figure(CALL)}
      <figcaption>
        The accent box is the only place thermodynamic numbers come from. Everything above it decides where to evaluate
        the model; everything below it reads the result. <code>Fluid</code> converts the input to molar units in the model's
        native gauge exactly once, so models never re-validate an input or see a reference state.
      </figcaption>
    </figure>
  </section>

  <section class="card part" data-testid="pairs">
    <h2>Nineteen pairs, three primitives</h2>
    <p>
      Each pair runs a fixed, ordered list of strategies and records the winner in <code>state.path()</code>. Most outer
      searches iterate over complete DT, PT or QT states, so the phase rule and the acceptance gate run inside them. The
      isobar first tries Newton in (T, ln ρ) directly on bundles and falls back to TOMS 748 over PT density solves.
    </p>
    <figure>
      {@render figure(PAIRS_FIGURE)}
      <figcaption>
        Pairs on the left, the search each one runs in the middle, the primitive every iterate comes down to on the
        right. D+X, T+X and P+X reach the saturation curve through the phase rule when a root lies inside the dome. HS
        and SU nest two deep: each isentrope iterate is a whole PS flash.
      </figcaption>
    </figure>
  </section>

  <section class="card part">
    <h2>The phase rule</h2>
    <p>
      One rule decides the phase for every pair. This is DT's version. PT asks the same questions with p against p<sub
        >sat</sub
      >(T) and then solves for density on the branch it picks; at p<sub>sat</sub> exactly it solves both and leaves the
      choice to <code>RootPolicy</code>.
    </p>
    <figure>
      {@render figure(PHASE)}
      <figcaption>
        The critical point that labels states is the model's own: the top of an exact saturation curve, else the
        published one. Below T<sub>c</sub>, with no fit covering T, the generic VLE of the EOS decides. A fit is never
        evaluated outside its range, under any <code>DomainPolicy</code>.
      </figcaption>
    </figure>
  </section>

  <section class="card part">
    <h2>Batches</h2>
    <p>
      <code>batch::evaluate</code> sends each point through <code>Input::new</code> and <code>Fluid::flash</code>, the
      scalar path, into caller-owned buffers laid out point by point. <code>ExecPolicy::Simd</code> changes one thing
      for DT, QT and PQ: the model evaluation moves out of the per-point loop and runs across 64 points in SIMD lanes.
      The
      <a href="#/benchmark">benchmark</a> times this batch on both of the page's builds.
    </p>
    <figure>
      {@render figure(BATCH)}
      <figcaption>
        Both policies give the same bits; <code>policy_equivalence</code> checks every cell and status against
        <code>Reference</code>. Branchy pairs (PH, HS, the Q+X searches) stay scalar per point.
        <code>ExecPolicy::Parallel</code> spreads points over threads in fixed chunks, so results do not depend on the thread
        count.
      </figcaption>
    </figure>
  </section>

  <section class="card part">
    <h2>What loads when</h2>
    <div class="table-frame">
      <table>
        <thead>
          <tr><th>When</th><th>What materialises</th><th>Held in</th></tr>
        </thead>
        <tbody>
          <tr>
            <td>Registry built</td>
            <td>Name, alias and CAS index (556 keys) and each fluid's resolved references</td>
            <td>Immutable layer</td>
          </tr>
          <tr>
            <td>First <code>get</code></td>
            <td>Blob validated, corrections applied, EOS compiled, limits and critical point</td>
            <td>Slot <code>OnceLock</code></td>
          </tr>
          <tr>
            <td>First saturation use</td>
            <td>Saturation curve; the superancillary is about 90 % of the source data</td>
            <td><code>LazyLock</code> in <code>PureFluid</code></td>
          </tr>
          <tr>
            <td>First transport call</td>
            <td>Transport set and its ECS reference fluids</td>
            <td><code>LazyLock</code> in <code>PureFluid</code></td>
          </tr>
        </tbody>
      </table>
    </div>
  </section>

  <section class="card part">
    <h2>Mixtures</h2>
    <p>
      <code>phasekit-mix</code> supplies a mixture as another <code>ThermoModel</code>, reached by name through
      <code>Registry::resolve("A[x]&amp;B[y]")</code>. Its own flash replaces the pure-fluid box in the first figure: so
      far PT by Michelsen's stability test and split, and bubble and dew points for QT and PQ at Q = 0 and 1. It returns
      a
      <code>State</code> built with <code>from_split</code>, so everything below the flash is unchanged. Mixtures are
      not on this site yet.
    </p>
  </section>

  <p class="source">
    Drawn from the source on 2026-10-17:
    {#each FILES as file, i (file)}<a href="{SOURCE}{file}"><code>{file}</code></a>{i < FILES.length - 1
        ? ", "
        : ""}{/each}
    in <code>crates/phasekit-core/src/</code>, and <code>docs/ARCHITECTURE.md</code> §5–8.
  </p>
</article>

<style>
  .how {
    display: grid;
    gap: 20px;
    margin: 36px 0;
  }

  .part {
    display: grid;
    gap: 12px;
    padding: 22px 24px;
    min-width: 0;
  }

  h1 {
    margin: 0;
    font-size: 26px;
  }

  h2 {
    margin: 0;
    font-size: 19px;
  }

  p {
    margin: 0;
    max-width: 72ch;
  }

  .lede {
    font-size: 16px;
  }

  .legend {
    list-style: none;
    margin: 4px 0 0;
    padding: 0;
    display: flex;
    flex-wrap: wrap;
    gap: 8px 22px;
    font-size: 13px;
    color: var(--fg-muted);
  }

  .legend li {
    display: inline-flex;
    align-items: center;
    gap: 8px;
  }

  .legend svg {
    width: 34px;
    height: 12px;
    flex: none;
  }

  figure {
    margin: 0;
    display: grid;
    gap: 10px;
    min-width: 0;
  }

  .frame {
    overflow-x: auto;
    background: var(--surface-2);
    border-radius: var(--radius-sm);
    padding: 12px;
  }

  .frame svg {
    display: block;
    width: 100%;
    height: auto;
  }

  figcaption,
  .source {
    font-size: 13.5px;
    color: var(--fg-muted);
    max-width: 80ch;
  }

  svg text {
    font-family: var(--font);
    font-size: 11.5px;
    fill: var(--fg-muted);
  }

  svg .title {
    font-size: 13px;
    font-weight: 600;
    fill: var(--fg);
  }

  svg .title.model,
  svg .accent {
    fill: var(--accent);
  }

  svg .code {
    font-family: var(--mono);
    font-size: 11px;
  }

  svg .title.code {
    font-size: 12px;
  }

  svg .label {
    font-size: 11px;
    paint-order: stroke;
    stroke: var(--surface-2);
    stroke-width: 4px;
    stroke-linejoin: round;
  }

  svg .caps {
    font-size: 10.5px;
    font-weight: 600;
    letter-spacing: 0.08em;
  }

  svg .small {
    font-size: 10.5px;
  }

  svg .chip-label {
    font-family: var(--mono);
    font-size: 12px;
    fill: var(--fg);
  }

  .box,
  .chip {
    fill: var(--surface);
    stroke: var(--border-strong);
  }

  .box.model {
    fill: color-mix(in srgb, var(--accent) 10%, var(--surface));
    stroke: var(--accent);
    stroke-width: 1.6;
  }

  .box.lazy {
    fill: color-mix(in srgb, var(--iso-1) 8%, var(--surface));
    stroke: var(--iso-1);
    stroke-dasharray: 5 3;
  }

  .box.refused {
    fill: none;
    stroke: var(--fg-muted);
    stroke-dasharray: 2 3;
  }

  .group {
    fill: none;
    stroke: var(--border);
    stroke-width: 1.2;
  }

  .loop {
    fill: none;
    stroke: var(--fg-muted);
    stroke-dasharray: 6 4;
  }

  .edge {
    fill: none;
    stroke: var(--fg-muted);
    stroke-width: 1.3;
  }

  .edge.model {
    stroke: var(--accent);
    stroke-width: 1.6;
  }

  .edge.lazy {
    stroke: var(--iso-1);
    stroke-dasharray: 5 3;
  }

  .edge.refused {
    stroke-dasharray: 2 3;
  }

  .tip {
    fill: var(--fg-muted);
  }

  .tip.model {
    fill: var(--accent);
  }

  .tip.lazy {
    fill: var(--iso-1);
  }

  .table-frame {
    overflow-x: auto;
  }

  table {
    border-collapse: collapse;
    width: 100%;
    min-width: 560px;
    font-size: 14px;
  }

  th,
  td {
    text-align: left;
    padding: 9px 12px;
    border-bottom: 1px solid var(--border);
    vertical-align: top;
  }

  th {
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--fg-muted);
  }

  tbody tr:last-child td {
    border-bottom: 0;
  }

  td:first-child {
    white-space: nowrap;
    font-weight: 550;
  }

  @media (max-width: 560px) {
    .part {
      padding: 16px;
    }
  }
</style>

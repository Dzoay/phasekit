<script lang="ts">
  // The surfaces (PLAN.md M26.2; user decisions DG7, DG8): a fluid's p–v–T or u–s–v surface, the package's mesh, drawn
  // in WebGL with three.js, coloured by phase, the dome and the critical point marked, turned with the mouse. Its view
  // is src/surface/view.svelte.ts, its placement src/surface/mesh.ts and its scene src/surface/scene.ts.
  import FluidOrigin from "../components/FluidOrigin.svelte";
  import FluidPicker from "../components/FluidPicker.svelte";
  import { PHASES } from "../diagram/style";
  import { getEngine } from "../engine/client";
  import { formatNumber } from "../lib/format";
  import { route, writeParams } from "../lib/route.svelte";
  import {
    KINDS,
    domePath,
    phasesPresent,
    positions,
    surfaceToken,
    vertexColours,
    type AxisKey,
  } from "../surface/mesh";
  import { SurfaceScene, linearRgb } from "../surface/scene";
  import { SurfaceView } from "../surface/view.svelte";

  const view = new SurfaceView(getEngine());
  $effect(() => view.connect(route, (params) => writeParams("surfaces", params)));

  const AXES: Record<AxisKey, { name: string; unit: string }> = {
    v: { name: "Specific volume v", unit: "m³/kg" },
    p: { name: "Pressure p", unit: "Pa" },
    T: { name: "Temperature T", unit: "K" },
    u: { name: "Internal energy u", unit: "J/kg" },
    s: { name: "Entropy s", unit: "J/(kg·K)" },
  };
  const PLACES = ["Across", "Up", "Deep"] as const;

  let box: HTMLElement | undefined = $state();
  let canvas: HTMLCanvasElement | undefined = $state();
  let probe: HTMLSpanElement | undefined = $state();
  let scene: SurfaceScene | undefined = $state();
  let width = $state(800);
  const height = $derived(Math.round(Math.min(640, Math.max(320, width * 0.66))));
  let theme = $state(0);

  /** A colour token resolved in the current theme. */
  function colour(token: string): string {
    if (!probe) return "currentColor";
    probe.style.color = `var(${token})`;
    return getComputedStyle(probe).color;
  }

  // The scene lives as long as its canvas.
  $effect(() => {
    if (!canvas) return;
    const made = new SurfaceScene(canvas);
    scene = made;
    return () => {
      made.dispose();
      scene = undefined;
    };
  });

  // The canvas's width follows its card.
  $effect(() => {
    if (!box) return;
    const observer = new ResizeObserver(([entry]) => {
      if (entry) width = Math.max(320, Math.round(entry.contentRect.width));
    });
    observer.observe(box);
    return () => observer.disconnect();
  });

  $effect(() => scene?.resize(width, height));

  // A new theme, or the system's dark mode turning on or off, re-reads the colours.
  $effect(() => {
    const bump = () => (theme += 1);
    const attributes = new MutationObserver(bump);
    attributes.observe(document.documentElement, { attributes: true, attributeFilter: ["data-theme", "data-accent"] });
    const dark = matchMedia("(prefers-color-scheme: dark)");
    dark.addEventListener("change", bump);
    return () => {
      attributes.disconnect();
      dark.removeEventListener("change", bump);
    };
  });

  const placed = $derived(view.data ? positions(view.data, view.kind) : null);
  const present = $derived(view.data ? phasesPresent(view.data, Object.keys(PHASES)) : []);

  $effect(() => {
    void theme;
    const m = view.data;
    if (!scene || !m || !placed) return;
    const tint = (phase: string) => linearRgb(colour(surfaceToken(phase)));
    scene.draw({
      xyz: placed.xyz,
      colours: vertexColours(m, tint),
      triangles: m.triangles,
      dome: domePath(m),
      critical: m.critical[0] ?? null,
      line: colour("--fg"),
      box: colour("--border"),
    });
  });
</script>

<section class="intro">
  <h1>Surfaces</h1>
  <p class="muted">
    A fluid's thermodynamic surface in three dimensions, p–v–T or Gibbs's u–s–v, every vertex a state computed by
    phasekit's WebAssembly package here in your browser. Drag to turn it, scroll or pinch to zoom.
  </p>
</section>

<div class="card controls">
  <div class="field">
    <label for="surface-fluid">Fluid</label>
    <FluidPicker id="surface-fluid" bind:value={view.fluid} />
  </div>
  <fieldset class="field">
    <legend>Surface</legend>
    <div class="segmented">
      {#each Object.entries(KINDS) as [id, kind] (id)}
        <label class:active={view.kind === id}>
          <input
            type="radio"
            name="surface"
            checked={view.kind === id}
            onchange={() => view.choose(id as keyof typeof KINDS)}
          />
          {kind.name}
        </label>
      {/each}
    </div>
  </fieldset>
  <button type="button" class="reset" onclick={() => scene?.reset()}>Reset view</button>
</div>

<section
  class="card plot"
  bind:this={box}
  aria-busy={view.busy}
  data-testid="surface"
  data-state={view.busy ? "busy" : view.refusal ? "refused" : "ready"}
>
  <canvas
    bind:this={canvas}
    style:width="{width}px"
    style:height="{height}px"
    aria-label="The {KINDS[view.kind].name} surface of {view.fluid}"
    data-vertices={view.data?.T.length ?? 0}
  ></canvas>
  {#if view.refusal}
    <p class="refusal" role="alert">{view.refusal}</p>
  {:else if view.busy}
    <p class="status muted">Computing the surface…</p>
  {/if}
</section>

{#if view.data && placed}
  <div class="below">
    <section class="card legend">
      <h2>Axes</h2>
      <table>
        <tbody>
          {#each placed.axes as axis, k (axis.key)}
            <tr>
              <th scope="row">{PLACES[k]}</th>
              <td>{AXES[axis.key].name}{axis.log ? ", logarithmic" : ""}</td>
              <td class="span">
                {formatNumber(axis.span[0])} to {formatNumber(axis.span[1])}
                {AXES[axis.key].unit}
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    </section>
    <section class="card legend">
      <h2>Phases</h2>
      <ul>
        {#each present as phase (phase)}
          <li><span class="swatch" style:background="var({surfaceToken(phase)})"></span>{PHASES[phase]?.name}</li>
        {/each}
        <li>
          <span class="swatch line"></span>The saturation dome{view.data.critical.length ? " and critical point" : ""}
        </li>
      </ul>
      <p class="muted small">{view.data.T.length} states, {view.data.triangles.length / 3} triangles.</p>
    </section>
  </div>
{/if}

<div class="fluid-data"><FluidOrigin fluid={view.fluid} /></div>

<span class="probe" bind:this={probe} aria-hidden="true"></span>

<style>
  .intro {
    max-width: 760px;
    margin: 36px 0 20px;
  }

  .intro h1 {
    font-size: clamp(26px, 4vw, 36px);
    margin: 0 0 10px;
    font-weight: 680;
  }

  .intro p {
    margin: 0;
    font-size: 16px;
  }

  .controls {
    --picker-padding: 6px 10px;
    --picker-height: 35px;
    display: flex;
    flex-wrap: wrap;
    gap: 16px 24px;
    padding: 14px 18px;
    margin-bottom: 20px;
    align-items: end;
  }

  .field {
    display: grid;
    gap: 6px;
    border: 0;
    margin: 0;
    padding: 0;
    min-width: 0;
  }

  .field > label,
  legend {
    font-size: 12px;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--fg-muted);
    padding: 0;
  }

  .segmented {
    display: flex;
    flex-wrap: wrap;
    background: var(--surface-2);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    padding: 3px;
  }

  .segmented label {
    position: relative;
    padding: 4px 12px;
    border-radius: 6px;
    font-size: 13px;
    color: var(--fg-muted);
    cursor: pointer;
    white-space: nowrap;
  }

  .segmented label.active {
    background: var(--surface);
    color: var(--fg);
    box-shadow: var(--shadow);
  }

  .segmented input {
    position: absolute;
    inset: 0;
    opacity: 0;
    cursor: pointer;
  }

  .segmented label:has(:focus-visible) {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }

  .reset {
    font: inherit;
    font-size: 13px;
    padding: 6px 12px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--surface);
    color: var(--fg);
    cursor: pointer;
  }

  .plot {
    position: relative;
    padding: 0;
    overflow: hidden;
  }

  canvas {
    display: block;
    touch-action: none;
    cursor: grab;
  }

  .status,
  .refusal {
    position: absolute;
    inset: auto 0 0 0;
    margin: 0;
    padding: 12px 18px;
  }

  .refusal {
    color: var(--danger);
  }

  .below {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(280px, 1fr));
    gap: 20px;
    margin-top: 20px;
  }

  .legend {
    padding: 14px 18px;
  }

  .legend h2 {
    font-size: 15px;
    margin: 0 0 10px;
  }

  .legend table {
    border-collapse: collapse;
    font-size: 13px;
    width: 100%;
  }

  .legend th {
    text-align: left;
    color: var(--fg-muted);
    font-weight: 600;
    padding: 3px 12px 3px 0;
  }

  .legend td {
    padding: 3px 12px 3px 0;
  }

  .legend .span {
    font-variant-numeric: tabular-nums;
  }

  .legend ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 6px;
    font-size: 13px;
  }

  .legend li {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .swatch {
    width: 14px;
    height: 14px;
    border-radius: 3px;
    border: 1px solid var(--border);
  }

  .swatch.line {
    height: 2px;
    border: 0;
    background: var(--fg);
  }

  .small {
    font-size: 12px;
    margin: 10px 0 0;
  }

  .fluid-data {
    margin-top: 16px;
  }

  .probe {
    display: none;
  }
</style>

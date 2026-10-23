<script lang="ts">
  // The characteristic curves (PLAN.md M26.6; user decision DG10): a fluid's ideal, Boyle, Joule–Thomson inversion and
  // Joule inversion curves whole, log p against log T up to the equation of state's highest temperature, over its
  // saturation line; and its compressibility chart. Every point is the package's. Its view is
  // src/curves/view.svelte.ts, its charts src/curves/chart.ts.
  import FluidPicker from "../components/FluidPicker.svelte";
  import LineChart from "../components/LineChart.svelte";
  import { curvesChart, frame, zChart } from "../curves/chart";
  import { CurvesView } from "../curves/view.svelte";
  import { CURVES } from "../diagram/overlays";
  import { getEngine } from "../engine/client";
  import { formatNumber } from "../lib/format";
  import { say } from "../lib/refusal";
  import { route, writeParams } from "../lib/route.svelte";

  const view = new CurvesView(getEngine());
  $effect(() => view.connect(route, (params) => writeParams("curves", params)));

  let box: HTMLElement | undefined = $state();
  let probe: HTMLSpanElement | undefined = $state();
  let width = $state(800);
  let theme = $state(0);

  /** A colour token resolved in the current theme. */
  function colour(token: string): string {
    if (!probe) return "currentColor";
    probe.style.color = `var(${token})`;
    return getComputedStyle(probe).color;
  }

  // The charts' width follows their card.
  $effect(() => {
    if (!box) return;
    const observer = new ResizeObserver(([entry]) => {
      if (entry) width = Math.max(320, Math.round(entry.contentRect.width));
    });
    observer.observe(box);
    return () => observer.disconnect();
  });

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

  const height = $derived(Math.round(Math.min(560, Math.max(300, width * 0.6))));
  const charts = $derived.by(() => {
    void theme;
    const d = view.data;
    if (!d || !probe) return null;
    return {
      curves: curvesChart(d.curves.curves, d.saturation, d.critical, frame(width, height), colour),
      z: zChart(d.curves.chart, frame(width, height), colour),
    };
  });
</script>

<section class="intro">
  <h1>Characteristic curves</h1>
  <p class="muted">
    Where a fluid behaves, in one respect, like an ideal gas: Z = 1 on the ideal curve, (∂Z/∂ρ)<sub>T</sub> = 0 on the
    Boyle curve, (∂T/∂p)<sub>h</sub> = 0 on the Joule–Thomson inversion curve and (∂Z/∂T)<sub>ρ</sub> = 0 on the Joule inversion
    curve, each traced by phasekit's WebAssembly package here in your browser up to the equation of state's highest temperature,
    and the compressibility chart beside them.
  </p>
</section>

<div class="card controls">
  <div class="field">
    <label for="curves-fluid">Fluid</label>
    <FluidPicker id="curves-fluid" bind:value={view.fluid} />
  </div>
</div>

<span class="probe" bind:this={probe} aria-hidden="true"></span>

<div class="card plot" bind:this={box} data-state={view.busy ? "loading" : view.refusal ? "refused" : "ready"}>
  {#if view.refusal}
    <p class="refusal" role="alert"><strong>Refused.</strong> {say(view.refusal, "si", "long")}</p>
  {:else if charts && view.data}
    <h2>The curves, log p against log T</h2>
    <LineChart
      chart={charts.curves}
      id="curves-chart"
      x="T / K"
      y="p / Pa"
      label="{view.fluid}'s characteristic curves on log p against log T"
    />
    <ul class="legend">
      <li>
        <svg width="28" height="10" aria-hidden="true"><line x1="0" x2="28" y1="5" y2="5" class="sat" /></svg>
        Saturation line and critical point
      </li>
      {#each view.data.curves.curves as c (c.name)}
        {@const s = CURVES[c.name]}
        {#if s}
          <li data-curve={c.name}>
            <svg width="28" height="10" aria-hidden="true">
              <line
                x1="0"
                x2="28"
                y1="5"
                y2="5"
                stroke="var({s.colour})"
                stroke-width="2"
                stroke-dasharray={s.dash.join(" ")}
              />
            </svg>
            {s.name}:
            {#if c.zeroDensity !== null}
              meets zero density at {formatNumber(c.zeroDensity)} K
            {:else}
              does not meet zero density below the equation of state's highest temperature
            {/if}
          </li>
        {/if}
      {/each}
    </ul>
    <h2>The compressibility chart</h2>
    <LineChart
      chart={charts.z}
      id="z-chart"
      x="p / Pa"
      y="Z"
      label="{view.fluid}'s compressibility factor against pressure, an isotherm a line"
    />
    <p class="muted note">
      Z = p/(ρRT) along each isotherm; below the critical temperature each breaks at its saturation pressure, between
      the saturated vapour and liquid.
    </p>
  {:else}
    <p class="muted">Tracing the curves…</p>
  {/if}
</div>

<style>
  .intro h1 {
    margin: 28px 0 6px;
  }

  .intro p {
    max-width: 72ch;
  }

  .controls {
    display: flex;
    flex-wrap: wrap;
    gap: 16px;
    padding: 14px 16px;
    margin: 16px 0;
  }

  .field {
    display: grid;
    gap: 4px;
    min-width: 220px;
  }

  .field label {
    font-size: 12.5px;
    color: var(--fg-muted);
  }

  .plot {
    padding: 14px 16px 18px;
    margin-bottom: 24px;
    min-height: 320px;
  }

  h2 {
    font-size: 14px;
    margin: 6px 0 8px;
    font-weight: 650;
  }

  .legend {
    list-style: none;
    margin: 10px 0 18px;
    padding: 0;
    display: grid;
    gap: 6px;
    font-size: 13.5px;
  }

  .legend li {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .sat {
    stroke: var(--fg);
    stroke-width: 2;
  }

  .note {
    font-size: 12.5px;
  }

  .probe {
    position: absolute;
    width: 0;
    height: 0;
    overflow: hidden;
  }

  .refusal strong {
    color: var(--danger);
  }
</style>

<script lang="ts">
  // The sweeps (PLAN.md M21.3): an output against T along isobars, against p along isotherms, or against T along
  // saturation, every point the package's. Its view and wiring are src/sweeps/view.svelte.ts and its chart's model
  // src/sweeps/chart.ts; this component draws the model in SVG and reads the samples under the cursor.
  import { pathOf } from "../diagram/model";
  import { getEngine } from "../engine/client";
  import { FLUIDS } from "../lib/fluids";
  import { formatNumber } from "../lib/format";
  import { GROUPS } from "../lib/outputs";
  import { route, writeParams } from "../lib/route.svelte";
  import { SYSTEMS, fromSI, toSI } from "../lib/units";
  import { LATENT, MODES, type SweepMode } from "../sample/sweeps";
  import { buildChart, nearest } from "../sweeps/chart";
  import { SweepView, heldUnit, unitOfOutput } from "../sweeps/view.svelte";

  const view = new SweepView(getEngine());
  $effect(() => view.connect(route, (params) => writeParams("sweeps", params)));

  const MODE_LABELS: Record<SweepMode, string> = {
    isobars: "Along isobars",
    isotherms: "Along isotherms",
    saturation: "Along saturation",
  };

  let box: HTMLDivElement | undefined = $state();
  let probe: HTMLSpanElement | undefined = $state();
  let width = $state(800);
  const height = $derived(Math.round(Math.min(560, Math.max(300, width * 0.56))));
  let theme = $state(0);
  /** The sample under the cursor, or -1. */
  let index = $state(-1);
  /** The held values' text: the view's, or what the viewer is typing. */
  let held = $state("");

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

  /** A colour token resolved in the current theme (`light-dark()` resolves only on an element). */
  function colour(token: string): string {
    if (!probe) return "currentColor";
    probe.style.color = `var(${token})`;
    return getComputedStyle(probe).color;
  }

  const chart = $derived.by(() => {
    void theme;
    return view.data ? buildChart(view.data, view.y, { width, height, system: view.system, colour }) : null;
  });

  // The held values' text follows the view.
  $effect(() => {
    held = view.heldText();
  });

  function move(event: PointerEvent): void {
    if (!chart || !view.data) return;
    const bounds = (event.currentTarget as SVGSVGElement).getBoundingClientRect();
    const px = event.clientX - bounds.left;
    if (px < chart.plot.left || px > chart.plot.right) return void (index = -1);
    index = nearest(view.data, toSI(chart.x.invert(px), unitOfOutput(view.data.x, view.system)));
  }

  const shown = (key: string, value: number | undefined) => {
    if (value === undefined) return "—";
    const unit = unitOfOutput(key, view.system);
    return `${formatNumber(fromSI(value, unit))} ${unit.label}`.trim();
  };

  // The sample under the cursor: its x, and each line's value there.
  const reading = $derived.by(() => {
    if (!view.data || !chart || index < 0) return null;
    const lines = view.data.lines.map((line, i) => ({ line: chart.lines[i], point: line.points[index] }));
    const x = lines.find((l) => l.point)?.point?.x;
    return { x, lines };
  });
</script>

<section class="intro">
  <h1>Sweeps</h1>
  <p class="muted">
    Any of the package's outputs along isobars, along isotherms or along saturation, every point computed by phasekit's
    WebAssembly package here in your browser. Move over the plot to read each line's value.
  </p>
</section>

<div class="card controls">
  <label class="field">
    <span>Fluid</span>
    <select bind:value={view.fluid}>
      {#each FLUIDS as f (f.name)}
        <option value={f.name}>{f.label}</option>
      {/each}
    </select>
  </label>
  <fieldset class="field">
    <legend>Sweep</legend>
    <div class="segmented">
      {#each MODES as m (m)}
        <label class:active={view.mode === m}>
          <input type="radio" name="sweep-mode" checked={view.mode === m} onchange={() => view.setMode(m)} />
          {MODE_LABELS[m]}
        </label>
      {/each}
    </div>
  </fieldset>
  <label class="field">
    <span>Output</span>
    <select value={view.y} onchange={(e) => (view.y = e.currentTarget.value)} data-testid="output">
      {#if view.mode === "saturation"}
        <option value={LATENT}>Latent heat, h″ − h′</option>
      {/if}
      {#each GROUPS as g (g.title)}
        <optgroup label={g.title}>
          {#each g.outputs as o (o.key)}
            <option value={o.key}>{o.label} ({o.symbol})</option>
          {/each}
        </optgroup>
      {/each}
    </select>
  </label>
  {#if view.mode !== "saturation"}
    <label class="field">
      <span>{view.mode === "isobars" ? "Pressures" : "Temperatures"}, {heldUnit(view.mode, view.system).label}</span>
      <input
        class="held"
        value={held}
        oninput={(e) => (held = e.currentTarget.value)}
        onchange={() => view.setValues(held)}
        data-testid="held"
      />
    </label>
  {/if}
  <fieldset class="field">
    <legend>Units</legend>
    <div class="segmented">
      {#each SYSTEMS as s (s.id)}
        <label class:active={view.system === s.id}>
          <input type="radio" name="sweep-units" checked={view.system === s.id} onchange={() => view.setSystem(s.id)} />
          {s.label}
        </label>
      {/each}
    </div>
  </fieldset>
</div>

<div class="layout">
  <div
    class="card plot"
    bind:this={box}
    data-testid="sweep"
    data-state={view.busy ? "loading" : view.refusal ? "refused" : "ready"}
  >
    {#if view.refusal}
      <div class="refusal" role="alert">
        <strong>The package refused this sweep.</strong>
        <p><code>{view.refusal}</code></p>
      </div>
    {:else}
      <div class="stack" style:height="{height}px">
        <svg
          {width}
          {height}
          role="img"
          aria-label="{view.y} of {view.fluid} {MODE_LABELS[view.mode].toLowerCase()}"
          onpointermove={move}
          onpointerleave={() => (index = -1)}
        >
          {#if chart}
            {@const p = chart.plot}
            <defs>
              <clipPath id="sweep-clip"
                ><rect x={p.left} y={p.top} width={p.right - p.left} height={p.bottom - p.top} /></clipPath
              >
            </defs>
            <rect class="frame" x={p.left} y={p.top} width={p.right - p.left} height={p.bottom - p.top} />
            {#each chart.xTicks as t (t.at)}
              <line class="tick" x1={t.at} x2={t.at} y1={p.bottom} y2={p.bottom + 5} />
              <text class="tick-text" x={t.at} y={p.bottom + 18} text-anchor="middle">{t.text}</text>
            {/each}
            {#each chart.yTicks as t (t.at)}
              <line class="grid" x1={p.left} x2={p.right} y1={t.at} y2={t.at} />
              <text class="tick-text" x={p.left - 8} y={t.at + 4} text-anchor="end">{t.text}</text>
            {/each}
            <text class="title" x={(p.left + p.right) / 2} y={height - 8} text-anchor="middle">{chart.titles.x}</text>
            <text class="title" transform="translate(16 {(p.top + p.bottom) / 2}) rotate(-90)" text-anchor="middle">
              {chart.titles.y}
            </text>
            <g clip-path="url(#sweep-clip)">
              {#each chart.lines as line, i (i)}
                {#each line.segments as segment, j (j)}
                  <path
                    class="line"
                    d={pathOf(segment)}
                    stroke={line.colour}
                    stroke-dasharray={line.dash.join(" ")}
                    data-testid="sweep-line"
                  />
                {/each}
              {/each}
              {#each chart.labels as l, i (i)}
                <text class="label" x={l.x} y={l.y} text-anchor={l.anchor}>{l.text}</text>
              {/each}
              {#if reading && view.data}
                {@const x = reading.x}
                {#if x !== undefined}
                  {@const px = chart.x(fromSI(x, unitOfOutput(view.data.x, view.system)))}
                  <line class="crosshair" x1={px} x2={px} y1={p.top} y2={p.bottom} />
                  {#each reading.lines as r, i (i)}
                    {#if r.point && r.line}
                      <circle
                        cx={px}
                        cy={chart.y(fromSI(r.point.y, unitOfOutput(view.y, view.system)))}
                        r="4"
                        fill={r.line.colour}
                        class="dot"
                      />
                    {/if}
                  {/each}
                {/if}
              {/if}
            </g>
          {/if}
        </svg>
        {#if view.busy}
          <p class="loading">Sweeping {view.fluid}…</p>
        {/if}
      </div>
    {/if}
  </div>

  <aside class="side">
    <section class="card readout" data-testid="sweep-readout">
      <h2>Under the cursor</h2>
      <p class="status">
        {#if reading && view.data}
          {view.data.x === "T" ? "T" : "p"} = <strong>{shown(view.data.x, reading.x)}</strong>
        {:else}
          Move over the plot.
        {/if}
      </p>
      <table>
        <tbody>
          {#each chart?.lines ?? [] as line, i (i)}
            {@const point = reading?.lines[i]?.point}
            <tr>
              <th scope="row">
                <svg width="22" height="10" aria-hidden="true">
                  <line
                    x1="0"
                    x2="22"
                    y1="5"
                    y2="5"
                    stroke={line.colour}
                    stroke-width="2"
                    stroke-dasharray={line.dash.join(" ")}
                  />
                </svg>
                {line.name}
              </th>
              <td
                class="num"
                data-testid="sweep-value"
                data-value={point ? String(point.y) : undefined}
                data-inputs={point ? JSON.stringify(point.inputs) : undefined}
              >
                {shown(view.y, point?.y)}
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    </section>
  </aside>
</div>

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

  .field > span,
  legend {
    font-size: 12px;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--fg-muted);
    padding: 0;
  }

  select,
  .held {
    background: var(--surface);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-sm);
    padding: 6px 10px;
    min-width: 12em;
  }

  .held {
    min-width: 22em;
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
    margin: 0;
    cursor: pointer;
  }

  .segmented label:has(:focus-visible) {
    outline: 2px solid var(--accent);
  }

  .layout {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 280px;
    gap: 20px;
    align-items: start;
  }

  .plot {
    padding: 8px;
    min-width: 0;
  }

  .stack {
    position: relative;
  }

  .stack svg {
    position: absolute;
    inset: 0;
    cursor: crosshair;
  }

  .frame {
    fill: none;
    stroke: var(--border-strong);
  }

  .tick {
    stroke: var(--border-strong);
  }

  .grid {
    stroke: var(--border);
  }

  .tick-text,
  .label {
    fill: var(--fg-muted);
    font-size: 11px;
    font-variant-numeric: tabular-nums;
  }

  .title {
    fill: var(--fg);
    font-size: 12.5px;
    font-weight: 550;
  }

  .line {
    fill: none;
    stroke-width: 2;
    stroke-linejoin: round;
    stroke-linecap: round;
  }

  .crosshair {
    stroke: var(--fg-muted);
    stroke-dasharray: 3 3;
  }

  .dot {
    stroke: var(--surface);
    stroke-width: 2;
  }

  .loading {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    margin: 0;
    color: var(--fg-muted);
    background: color-mix(in srgb, var(--surface) 70%, transparent);
  }

  .refusal {
    padding: 16px 18px;
    border: 1px solid var(--danger);
    border-radius: var(--radius-sm);
  }

  .refusal strong {
    color: var(--danger);
  }

  .side {
    display: grid;
    gap: 16px;
  }

  .side .card {
    padding: 14px 16px;
  }

  h2 {
    font-size: 14px;
    margin: 0 0 8px;
    font-weight: 650;
  }

  .readout table {
    width: 100%;
    border-collapse: collapse;
    font-size: 13.5px;
  }

  .readout th {
    text-align: left;
    font-weight: 450;
    padding: 3px 0;
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .readout td {
    text-align: right;
    padding: 3px 0;
  }

  .status {
    margin: 0 0 8px;
    font-size: 13px;
    line-height: 20px;
    color: var(--fg-muted);
  }

  .status strong {
    line-height: 1;
    font-family: var(--mono);
    color: var(--fg);
  }

  .probe {
    display: none;
  }

  @media (max-width: 900px) {
    .layout {
      grid-template-columns: 1fr;
    }
  }
</style>

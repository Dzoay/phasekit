<script lang="ts">
  // The diagrams (PLAN.md M21.2): a fluid's saturation dome and isolines on one of five planes, the state under the
  // cursor, zoom and pan, and the diagram as CSV, SVG or PNG. Its view and wiring are src/diagram/view.svelte.ts and
  // its drawing model src/diagram/model.ts; this component binds to them, paints the model's lines on a canvas and its
  // axes, labels and crosshair in SVG (user decision WS2).
  import { select } from "d3-selection";
  import { zoom, zoomIdentity, type ZoomBehavior, type ZoomTransform } from "d3-zoom";
  import { untrack } from "svelte";
  import { buildModel, renderSvg, toCsv } from "../diagram/model";
  import { baseScales, readoutInputs } from "../diagram/scales";
  import { styleOf, unitOfKey } from "../diagram/style";
  import { DiagramView, READOUT_KEYS } from "../diagram/view.svelte";
  import { getEngine } from "../engine/client";
  import { FLUIDS } from "../lib/fluids";
  import { formatNumber } from "../lib/format";
  import { output } from "../lib/outputs";
  import { route, writeParams } from "../lib/route.svelte";
  import { SYSTEMS, fromSI } from "../lib/units";
  import { DIAGRAMS } from "../sample/diagrams";

  const view = new DiagramView(getEngine());
  $effect(() => view.connect(route, (params) => writeParams("diagrams", params)));

  let box: HTMLDivElement | undefined = $state();
  let canvas: HTMLCanvasElement | undefined = $state();
  let overlay: SVGSVGElement | undefined = $state();
  let probe: HTMLSpanElement | undefined = $state();
  let width = $state(800);
  const height = $derived(Math.round(Math.min(620, Math.max(320, width * 0.62))));
  let transform = $state<ZoomTransform>(zoomIdentity);
  let theme = $state(0);
  let cursor = $state<[number, number] | null>(null);
  let zoomer: ZoomBehavior<SVGSVGElement, unknown> | undefined;

  // The plot's width follows its card.
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

  const scales = $derived(view.data ? baseScales(view.diagram, view.data, width, height, view.system) : null);
  const zoomed = $derived(
    scales ? { x: transform.rescaleX(scales.x), y: transform.rescaleY(scales.y), plot: scales.plot } : null,
  );
  const model = $derived.by(() => {
    void theme;
    if (!view.data || !zoomed) return null;
    const hidden = new Set(view.hidden);
    return buildModel(view.diagram, view.data, { ...zoomed, system: view.system, hidden, colour });
  });

  // Paint the model's lines and the critical point, clipped to the plot.
  $effect(() => {
    const context = canvas?.getContext("2d");
    if (!canvas || !context) return;
    const ratio = devicePixelRatio || 1;
    [canvas.width, canvas.height] = [Math.round(width * ratio), Math.round(height * ratio)];
    context.setTransform(ratio, 0, 0, ratio, 0, 0);
    context.clearRect(0, 0, width, height);
    if (!model) return;
    const { left, top, right, bottom } = model.plot;
    context.save();
    context.beginPath();
    context.rect(left, top, right - left, bottom - top);
    context.clip();
    [context.lineJoin, context.lineCap] = ["round", "round"];
    for (const stroke of model.strokes) {
      context.beginPath();
      context.setLineDash([...stroke.dash]);
      [context.lineWidth, context.strokeStyle] = [stroke.width, stroke.colour];
      stroke.points.forEach(([x, y], i) => (i === 0 ? context.moveTo(x, y) : context.lineTo(x, y)));
      context.stroke();
    }
    context.setLineDash([]);
    const surface = colour("--surface");
    for (const dot of model.dots) {
      for (const [r, fill] of [
        [dot.r + 2, surface],
        [dot.r, dot.colour],
      ] as const) {
        context.beginPath();
        context.arc(dot.x, dot.y, r, 0, 2 * Math.PI);
        context.fillStyle = fill;
        context.fill();
      }
    }
    context.restore();
  });

  // Zoom and pan on the overlay; a double-click, a new diagram, fluid or units, start again from the whole diagram.
  $effect(() => {
    if (!overlay) return;
    const target = select(overlay);
    zoomer = zoom<SVGSVGElement, unknown>()
      .scaleExtent([1, 40])
      .on("zoom", (event: { transform: ZoomTransform }) => (transform = event.transform));
    target.call(zoomer).on("dblclick.zoom", () => reset());
    return () => void target.on(".zoom", null);
  });

  function reset(): void {
    if (overlay && zoomer) select(overlay).call(zoomer.transform, zoomIdentity);
  }

  $effect(() => {
    void [view.id, view.fluid, view.system];
    untrack(reset);
  });

  function move(event: PointerEvent): void {
    if (!zoomed || !overlay) return;
    const bounds = overlay.getBoundingClientRect();
    const [px, py] = [event.clientX - bounds.left, event.clientY - bounds.top];
    const { left, top, right, bottom } = zoomed.plot;
    if (px < left || px > right || py < top || py > bottom) return leave();
    cursor = [px, py];
    view.point(readoutInputs(view.diagram, zoomed.x, zoomed.y, px, py, view.system));
  }

  function leave(): void {
    cursor = null;
    view.leave();
  }

  function download(name: string, blob: Blob): void {
    const link = document.createElement("a");
    link.href = URL.createObjectURL(blob);
    link.download = name;
    link.click();
    setTimeout(() => URL.revokeObjectURL(link.href), 1000);
  }

  const fileName = (extension: string) => `${view.fluid}-${view.id}.${extension}`;

  function svg(): string | null {
    if (!model) return null;
    const colours = { background: colour("--surface"), ink: colour("--fg"), muted: colour("--fg-muted") };
    return renderSvg(model, { width, height, ...colours, font: "Inter, system-ui, sans-serif" });
  }

  function exportCsv(): void {
    if (view.data) download(fileName("csv"), new Blob([toCsv(view.diagram, view.data)], { type: "text/csv" }));
  }

  function exportSvg(): void {
    const text = svg();
    if (text) download(fileName("svg"), new Blob([text], { type: "image/svg+xml" }));
  }

  async function exportPng(): Promise<void> {
    const text = svg();
    if (!text) return;
    const url = URL.createObjectURL(new Blob([text], { type: "image/svg+xml" }));
    const image = new Image();
    image.src = url;
    await image.decode();
    const sheet = document.createElement("canvas");
    [sheet.width, sheet.height] = [width * 2, height * 2];
    const context = sheet.getContext("2d");
    context?.scale(2, 2);
    context?.drawImage(image, 0, 0);
    URL.revokeObjectURL(url);
    sheet.toBlob((blob) => blob && download(fileName("png"), blob), "image/png");
  }

  const shown = (key: string, value: number | null | undefined) => {
    if (typeof value !== "number" || Number.isNaN(value)) return "—";
    const unit = unitOfKey(key as never, view.system);
    return `${formatNumber(fromSI(value, unit))} ${unit.label}`.trim();
  };
</script>

<section class="intro">
  <h1>Diagrams</h1>
  <p class="muted">
    A fluid's saturation dome and isolines, every point computed by phasekit's WebAssembly package here in your browser.
    Move over the plot to read the state under the cursor; scroll or drag to zoom and pan, double-click to start again.
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
    <legend>Diagram</legend>
    <div class="segmented">
      {#each DIAGRAMS as d (d.id)}
        <label class:active={view.id === d.id}>
          <input type="radio" name="diagram" checked={view.id === d.id} onchange={() => view.choose(d.id)} />
          {d.title}
        </label>
      {/each}
    </div>
  </fieldset>
  <fieldset class="field">
    <legend>Units</legend>
    <div class="segmented">
      {#each SYSTEMS as s (s.id)}
        <label class:active={view.system === s.id}>
          <input
            type="radio"
            name="diagram-units"
            checked={view.system === s.id}
            onchange={() => view.setSystem(s.id)}
          />
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
    data-testid="plot"
    data-state={view.busy ? "loading" : view.refusal ? "refused" : "ready"}
  >
    {#if view.refusal}
      <div class="refusal" role="alert">
        <strong>The package refused this diagram.</strong>
        <p><code>{view.refusal}</code></p>
      </div>
    {:else}
      <div class="stack" style:height="{height}px">
        <canvas bind:this={canvas} style:width="{width}px" style:height="{height}px"></canvas>
        <svg
          bind:this={overlay}
          {width}
          {height}
          role="img"
          aria-label="{view.diagram.title} diagram of {view.fluid}"
          onpointermove={move}
          onpointerleave={leave}
        >
          {#if model}
            {@const p = model.plot}
            <defs>
              <clipPath id="plot-clip"
                ><rect x={p.left} y={p.top} width={p.right - p.left} height={p.bottom - p.top} /></clipPath
              >
            </defs>
            <rect class="frame" x={p.left} y={p.top} width={p.right - p.left} height={p.bottom - p.top} />
            {#each model.xTicks as t (t.at)}
              <line class="tick" x1={t.at} x2={t.at} y1={p.bottom} y2={p.bottom + 5} />
              <text class="tick-text" x={t.at} y={p.bottom + 18} text-anchor="middle">{t.text}</text>
            {/each}
            {#each model.yTicks as t (t.at)}
              <line class="tick" x1={p.left - 5} x2={p.left} y1={t.at} y2={t.at} />
              <text class="tick-text" x={p.left - 8} y={t.at + 4} text-anchor="end">{t.text}</text>
            {/each}
            <text class="title" x={(p.left + p.right) / 2} y={height - 8} text-anchor="middle">{model.titles.x}</text>
            <text class="title" transform="translate(16 {(p.top + p.bottom) / 2}) rotate(-90)" text-anchor="middle">
              {model.titles.y}
            </text>
            <g clip-path="url(#plot-clip)">
              {#each model.labels as l, i (i)}
                <text class="label" x={l.x} y={l.y} text-anchor={l.anchor}>{l.text}</text>
              {/each}
              {#if cursor}
                <line class="crosshair" x1={cursor[0]} x2={cursor[0]} y1={p.top} y2={p.bottom} />
                <line class="crosshair" x1={p.left} x2={p.right} y1={cursor[1]} y2={cursor[1]} />
              {/if}
            </g>
          {/if}
        </svg>
        {#if view.busy}
          <p class="loading">Sampling {view.fluid}…</p>
        {/if}
      </div>
    {/if}
  </div>

  <aside class="side">
    <section class="card legend">
      <h2>Lines</h2>
      <ul>
        <li>
          <svg width="28" height="10" aria-hidden="true"><line x1="0" x2="28" y1="5" y2="5" class="swatch-dome" /></svg>
          Saturation dome and critical point
        </li>
        {#each view.diagram.families as f (f.held)}
          {@const s = styleOf(f.held)}
          <li>
            <label>
              <input type="checkbox" checked={!view.hidden.includes(f.held)} onchange={() => view.toggle(f.held)} />
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
              {s.name}
            </label>
          </li>
        {/each}
      </ul>
    </section>

    <section class="card readout" data-testid="readout" data-inputs={JSON.stringify(view.readout?.inputs ?? null)}>
      <h2>Under the cursor</h2>
      {#if view.readout?.state}
        <p class="phase">Phase <strong data-testid="readout-phase">{view.readout.state.phase}</strong></p>
        <table>
          <tbody>
            {#each READOUT_KEYS as key, i (key)}
              {@const v = view.readout.state.values[i]}
              <tr>
                <th scope="row">{output(key)?.label}</th>
                <td class="num" data-testid="readout-{key}" data-value={String(v)}>{shown(key, v)}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      {:else if view.readout?.refusal}
        <p class="muted">The package refused this point: <code>{view.readout.refusal}</code></p>
      {:else}
        <p class="muted">Move over the plot.</p>
      {/if}
    </section>

    <section class="card export">
      <h2>Export</h2>
      <div class="buttons">
        <button type="button" onclick={exportCsv} disabled={!view.data}>CSV</button>
        <button type="button" onclick={exportSvg} disabled={!model}>SVG</button>
        <button type="button" onclick={exportPng} disabled={!model}>PNG</button>
      </div>
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

  select {
    background: var(--surface);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-sm);
    padding: 6px 10px;
    min-width: 12em;
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

  canvas,
  .stack svg {
    position: absolute;
    inset: 0;
  }

  .stack svg {
    touch-action: none;
    cursor: crosshair;
  }

  .frame {
    fill: none;
    stroke: var(--border-strong);
  }

  .tick {
    stroke: var(--border-strong);
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

  .crosshair {
    stroke: var(--fg-muted);
    stroke-dasharray: 3 3;
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

  .legend ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 6px;
    font-size: 13.5px;
  }

  .legend li,
  .legend label {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .swatch-dome {
    stroke: var(--fg);
    stroke-width: 2;
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
  }

  .readout td {
    text-align: right;
    padding: 3px 0;
  }

  .phase {
    margin: 0 0 8px;
    font-size: 13px;
    color: var(--fg-muted);
  }

  .phase strong {
    font-family: var(--mono);
    color: var(--fg);
  }

  .buttons {
    display: flex;
    gap: 8px;
  }

  .buttons button {
    flex: 1;
    background: var(--surface-2);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-sm);
    padding: 6px 0;
    cursor: pointer;
  }

  .buttons button:hover:not(:disabled) {
    border-color: var(--accent);
    color: var(--accent);
  }

  .buttons button:disabled {
    cursor: default;
    opacity: 0.6;
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

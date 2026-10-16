<script lang="ts">
  // The diagrams (PLAN.md M21.2): a fluid's saturation dome and isolines on one of five planes, the state under the
  // cursor, zoom and pan, the phase regions (M21.9), and the diagram as CSV, SVG or PNG. Its view and wiring are src/diagram/view.svelte.ts and
  // its drawing model src/diagram/model.ts; this component binds to them, paints the model's lines on a canvas and its
  // axes, labels and crosshair in SVG (user decision WS2).
  import { select } from "d3-selection";
  import { zoom, zoomIdentity, type ZoomBehavior, type ZoomTransform } from "d3-zoom";
  import { untrack } from "svelte";
  import { buildModel, renderSvg, toCsv } from "../diagram/model";
  import { GRID_LEVELS, describeGrid, gridValues, snap, step } from "../diagram/grid";
  import { baseScales, readoutAt, shownAt } from "../diagram/scales";
  import { CRITICAL_LINE, PHASES, styleOf, unitOfKey } from "../diagram/style";
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
    const regions = view.regions ? view.phases : null;
    return buildModel(view.diagram, view.data, { ...zoomed, system: view.system, hidden, colour, regions });
  });

  // The snap grid in the shown units, over what the plot shows now (PLAN.md M21.8).
  const grids = $derived(
    zoomed
      ? {
          x: gridValues(zoomed.x.domain() as [number, number], view.diagram.x.log, view.grid),
          y: gridValues(zoomed.y.domain() as [number, number], view.diagram.y.log, view.grid),
        }
      : { x: [], y: [] },
  );

  // A pinned readout's crosshair sits on its state; otherwise it follows the cursor.
  const crosshair = $derived.by((): [number, number] | null => {
    if (!view.pinned) return cursor;
    if (!zoomed || !view.readout) return null;
    const [xv, yv] = shownAt(view.diagram, view.readout.inputs, view.system);
    return [zoomed.x(xv), zoomed.y(yv)];
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
    // The phase regions' tints under everything.
    for (const fill of model.fills) {
      context.beginPath();
      fill.points.forEach(([x, y], i) => (i === 0 ? context.moveTo(x, y) : context.lineTo(x, y)));
      context.closePath();
      context.fillStyle = fill.colour;
      context.fill();
    }
    // The snap grid under the lines, where it is not so fine as to fill the plot.
    if (view.snap && grids.x.length + grids.y.length <= 300) {
      [context.lineWidth, context.strokeStyle] = [1, colour("--border")];
      context.beginPath();
      for (const v of grids.x) {
        const x = Math.round(zoomedX(v)) + 0.5;
        context.moveTo(x, top);
        context.lineTo(x, bottom);
      }
      for (const v of grids.y) {
        const y = Math.round(zoomedY(v)) + 0.5;
        context.moveTo(left, y);
        context.lineTo(right, y);
      }
      context.stroke();
    }
    // Over the tints, each line on a halo of the surface, all the halos first so that none cuts a line.
    const passes = model.halo === null ? [null] : [model.halo, null];
    for (const halo of passes) {
      for (const stroke of model.strokes) {
        context.beginPath();
        context.setLineDash([...stroke.dash]);
        [context.lineWidth, context.strokeStyle] = halo ? [stroke.width + 2, halo] : [stroke.width, stroke.colour];
        stroke.points.forEach(([x, y], i) => (i === 0 ? context.moveTo(x, y) : context.lineTo(x, y)));
        context.stroke();
      }
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

  const zoomedX = (v: number) => (zoomed ? zoomed.x(v) : 0);
  const zoomedY = (v: number) => (zoomed ? zoomed.y(v) : 0);

  function move(event: PointerEvent): void {
    if (!zoomed || !overlay || view.pinned) return;
    const bounds = overlay.getBoundingClientRect();
    const [px, py] = [event.clientX - bounds.left, event.clientY - bounds.top];
    const { left, top, right, bottom } = zoomed.plot;
    if (px < left || px > right || py < top || py > bottom) return leave();
    let [xv, yv] = [zoomed.x.invert(px), zoomed.y.invert(py)];
    if (view.snap) [xv, yv] = [snap(xv, grids.x, view.diagram.x.log), snap(yv, grids.y, view.diagram.y.log)];
    cursor = [zoomed.x(xv), zoomed.y(yv)];
    view.hover(readoutAt(view.diagram, xv, yv, view.system));
  }

  function leave(): void {
    if (view.pinned) return;
    cursor = null;
    view.leave();
  }

  // A click pins the readout or lets it go; while pinned, the arrow keys step it along the grid and Esc lets it go.
  function click(event: MouseEvent): void {
    if (event.detail > 1) return;
    view.togglePin();
    if (!view.pinned) move(event as PointerEvent);
  }

  function key(event: KeyboardEvent): void {
    if (event.key === "Escape") return void view.unpin();
    const arrows: Record<string, [0 | 1, 1 | -1]> = {
      ArrowRight: [0, 1],
      ArrowLeft: [0, -1],
      ArrowUp: [1, 1],
      ArrowDown: [1, -1],
    };
    const arrow = arrows[event.key];
    if (!arrow || !view.pinned || !view.readout) return;
    event.preventDefault();
    const shown = shownAt(view.diagram, view.readout.inputs, view.system);
    const [axis, direction] = arrow;
    const [grid, log] = axis === 0 ? [grids.x, view.diagram.x.log] : [grids.y, view.diagram.y.log];
    shown[axis] = step(shown[axis], grid, log, direction);
    view.point(readoutAt(view.diagram, shown[0], shown[1], view.system));
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
    data-regions={view.regions ? (view.phases ? "ready" : "loading") : "off"}
  >
    {#if view.refusal}
      <div class="refusal" role="alert">
        <strong>The package refused this diagram.</strong>
        <p><code>{view.refusal}</code></p>
      </div>
    {:else}
      <div class="stack" style:height="{height}px">
        <canvas bind:this={canvas} style:width="{width}px" style:height="{height}px"></canvas>
        <!-- The plot takes its own pointer and keyboard input, as an ARIA application, its label saying how; Svelte's
             lint counts no role of an <svg> as interactive. -->
        <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
        <svg
          bind:this={overlay}
          {width}
          {height}
          role="application"
          aria-roledescription="diagram"
          aria-label="{view.diagram
            .title} diagram of {view.fluid}: move to read a state, click to pin it, arrow keys to step along the grid, Escape to let it go"
          tabindex="0"
          onpointermove={move}
          onpointerleave={leave}
          onclick={click}
          onkeydown={key}
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
              {#each model.regionLabels as l (l.phase)}
                <text
                  class="region-label"
                  data-testid="region-label"
                  data-phase={l.phase}
                  x={l.x}
                  y={l.y}
                  text-anchor="middle"
                  dominant-baseline="central">{l.text}</text
                >
              {/each}
              {#if crosshair}
                <line
                  class="crosshair"
                  class:pinned={view.pinned}
                  x1={crosshair[0]}
                  x2={crosshair[0]}
                  y1={p.top}
                  y2={p.bottom}
                />
                <line
                  class="crosshair"
                  class:pinned={view.pinned}
                  x1={p.left}
                  x2={p.right}
                  y1={crosshair[1]}
                  y2={crosshair[1]}
                />
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

    <section class="card phases">
      <h2>Phases</h2>
      <label class="check">
        <input type="checkbox" checked={view.regions} onchange={() => view.toggleRegions()} />
        Tint the phase regions
      </label>
      {#if view.regions}
        <ul>
          {#each Object.values(PHASES) as s (s.name)}
            <li><span class="swatch" style:background="var({s.colour})" aria-hidden="true"></span>{s.name}</li>
          {/each}
          <li>
            <svg width="28" height="10" aria-hidden="true">
              <line
                x1="0"
                x2="28"
                y1="5"
                y2="5"
                stroke="var({CRITICAL_LINE.colour})"
                stroke-width={CRITICAL_LINE.width}
                stroke-dasharray={CRITICAL_LINE.dash.join(" ")}
              />
            </svg>
            Critical isotherm and isobar
          </li>
        </ul>
      {/if}
    </section>

    <section class="card grid">
      <h2>Grid</h2>
      <label class="check">
        <input type="checkbox" checked={view.snap} onchange={() => view.toggleSnap()} />
        Snap to grid
      </label>
      <label class="density">
        <span>Density</span>
        <input
          type="range"
          min="1"
          max={GRID_LEVELS}
          step="1"
          value={view.grid}
          oninput={(e) => view.setGrid(Number(e.currentTarget.value))}
        />
      </label>
      {#if zoomed}
        {@const d = view.diagram}
        <p class="muted" data-testid="grid-description">
          {output(d.x.key)?.symbol}: {describeGrid(zoomed.x.domain() as [number, number], d.x.log, view.grid)}
          {d.x.log ? "" : unitOfKey(d.x.key, view.system).label} · {output(d.y.key)?.symbol}:
          {describeGrid(zoomed.y.domain() as [number, number], d.y.log, view.grid)}
          {d.y.log ? "" : unitOfKey(d.y.key, view.system).label}
        </p>
      {/if}
    </section>

    <section class="card readout" data-testid="readout" data-inputs={JSON.stringify(view.readout?.inputs ?? null)}>
      <h2>
        Under the cursor
        {#if view.pinned}<span class="pin" data-testid="pinned">pinned</span>{/if}
      </h2>
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
        {@const link = view.calculatorLink()}
        {#if link}
          <a class="open" href={link} data-testid="open-calculator">Open in calculator →</a>
        {/if}
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

  /* A region's name over its tint, on a halo of the surface where a line runs under it. */
  .region-label {
    fill: var(--fg);
    font-size: 12px;
    font-weight: 600;
    paint-order: stroke;
    stroke: var(--surface);
    stroke-width: 3px;
    stroke-linejoin: round;
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

  .crosshair.pinned {
    stroke: var(--accent);
    stroke-dasharray: none;
  }

  .stack svg:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: -2px;
  }

  .pin {
    margin-left: 6px;
    padding: 1px 7px;
    border-radius: 999px;
    background: var(--accent);
    color: var(--on-accent);
    font-size: 11px;
    font-weight: 600;
    vertical-align: middle;
  }

  .phases .check,
  .grid .check,
  .grid .density {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 13.5px;
    margin-bottom: 8px;
  }

  .grid .density input {
    flex: 1;
    accent-color: var(--accent);
  }

  .grid p {
    margin: 0;
    font-size: 12.5px;
  }

  .open {
    display: inline-block;
    margin-top: 10px;
    font-size: 13.5px;
    font-weight: 550;
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

  .phases ul,
  .legend ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 6px;
    font-size: 13.5px;
  }

  .phases li,
  .legend li,
  .legend label {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .phases .swatch {
    width: 28px;
    height: 14px;
    border-radius: 4px;
    border: 1px solid var(--border-strong);
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

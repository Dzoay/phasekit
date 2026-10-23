<script lang="ts">
  // The ternary diagram (PLAN.md M26.5, M26.6): a three-component mixture's two-phase region at one T and p on the
  // composition triangle, its boundary, the bubble and dew sides, and its tie lines, each the two phases of a feed
  // inside it; every point a feed the package's PT flash with the stability test classified. Its view is
  // src/ternary/view.svelte.ts, its placement src/ternary/geometry.ts.
  import { getEngine } from "../engine/client";
  import { pathOf } from "../diagram/model";
  import { formatNumber } from "../lib/format";
  import { say } from "../lib/refusal";
  import { route, writeParams } from "../lib/route.svelte";
  import { boxHeight, fractionsAt, gridLines, place, triangle, type Fractions } from "../ternary/geometry";
  import { TernaryView } from "../ternary/view.svelte";

  const view = new TernaryView(getEngine());
  $effect(() => view.connect(route, (params) => writeParams("ternary", params)));

  let box: HTMLElement | undefined = $state();
  let width = $state(720);
  let cursor = $state<Fractions | null>(null);

  $effect(() => {
    if (!box) return;
    const observer = new ResizeObserver(([entry]) => {
      if (entry) width = Math.max(320, Math.round(entry.contentRect.width));
    });
    observer.observe(box);
    return () => observer.disconnect();
  });

  const t = $derived(triangle(width));
  const at = (z: Fractions) => place(z, t);
  const corners = $derived([at([1, 0, 0]), at([0, 1, 0]), at([0, 0, 1])]);

  function move(event: PointerEvent): void {
    const r = (event.currentTarget as SVGSVGElement).getBoundingClientRect();
    cursor = fractionsAt(event.clientX - r.left, event.clientY - r.top, t);
  }

  const kPa = (p: number) => p / 1000;
</script>

<section class="intro">
  <h1>Ternary diagrams</h1>
  <p class="muted">
    A three-component mixture at one temperature and pressure: where on the composition triangle it splits into liquid
    and vapour, the bubble and dew sides of that region, and tie lines joining the two phases a feed inside it splits
    into. Every point is a feed classified by phasekit's WebAssembly package here in your browser, its PT flash with the
    stability test; a diagram takes a few seconds.
  </p>
</section>

<div class="card controls">
  <div class="field">
    <label for="ternary-mixture">Mixture</label>
    <select id="ternary-mixture" bind:value={view.mixture}>
      {#each view.choices.includes(view.mixture) ? view.choices : [view.mixture, ...view.choices] as name (name)}
        <option value={name}>{name.replace(/\.mix$/, "")}</option>
      {/each}
    </select>
  </div>
  <div class="field">
    <label for="ternary-t">T / K</label>
    <input
      id="ternary-t"
      type="number"
      step="any"
      value={view.T}
      onchange={(e) => view.set("T", Number(e.currentTarget.value))}
    />
  </div>
  <div class="field">
    <label for="ternary-p">p / kPa</label>
    <input
      id="ternary-p"
      type="number"
      step="any"
      value={kPa(view.p)}
      onchange={(e) => view.set("p", Number(e.currentTarget.value) * 1000)}
    />
  </div>
</div>

<div class="card plot" bind:this={box} data-state={view.busy ? "loading" : view.refusal ? "refused" : "ready"}>
  {#if view.refusal}
    <p class="refusal" role="alert"><strong>Refused.</strong> {say(view.refusal, "si", "long")}</p>
  {:else if view.data}
    {@const d = view.data}
    <!-- The pointer only reads the composition under it; the triangle is an image. -->
    <svg
      {width}
      height={boxHeight(t)}
      viewBox="0 0 {width} {boxHeight(t)}"
      role="img"
      aria-label="{d.components.join(', ')} at {formatNumber(d.T)} K and {formatNumber(kPa(d.p))} kPa"
      data-testid="ternary"
      data-pieces={d.region.length}
      data-lines={d.binodal.length}
      data-ties={d.ties.length}
      onpointermove={move}
      onpointerleave={() => (cursor = null)}
    >
      {#each d.region as piece, i (i)}
        <path class="region" d="{pathOf(piece.map(at))}Z" />
      {/each}
      {#each gridLines(t) as [a, b], i (i)}
        <line class="grid" x1={a[0]} y1={a[1]} x2={b[0]} y2={b[1]} />
      {/each}
      <path class="side" d="{pathOf(corners)}Z" />
      {#each d.ties as tie, i (i)}
        <path class="tie" d={pathOf([at(tie.liquid), at(tie.vapour)])} />
      {/each}
      {#each d.binodal as line, i (i)}
        <path class={line.bubble ? "bubble" : "dew"} d={pathOf(line.points.map(at))} />
      {/each}
      {#if d.feed}
        {@const [x, y] = at(d.feed)}
        <circle class="feed" cx={x} cy={y} r="5" data-testid="feed" />
      {/if}
      <text class="corner" x={corners[0]?.[0]} y={(corners[0]?.[1] ?? 0) + 24} text-anchor="end">{d.components[0]}</text
      >
      <text class="corner" x={corners[1]?.[0]} y={(corners[1]?.[1] ?? 0) - 12} text-anchor="middle"
        >{d.components[1]}</text
      >
      <text class="corner" x={corners[2]?.[0]} y={(corners[2]?.[1] ?? 0) + 24} text-anchor="start"
        >{d.components[2]}</text
      >
    </svg>
    <div class="below">
      <ul class="legend">
        <li><span class="swatch" aria-hidden="true"></span>Liquid + vapour</li>
        <li>
          <svg width="28" height="10" aria-hidden="true"><line x1="0" x2="28" y1="5" y2="5" class="bubble" /></svg>
          Bubble side: the feed nearly all liquid
        </li>
        <li>
          <svg width="28" height="10" aria-hidden="true"><line x1="0" x2="28" y1="5" y2="5" class="dew" /></svg>
          Dew side: the feed nearly all vapour
        </li>
        <li>
          <svg width="28" height="10" aria-hidden="true"><line x1="0" x2="28" y1="5" y2="5" class="tie" /></svg>
          Tie lines, liquid to vapour
        </li>
        <li>
          <svg width="28" height="10" aria-hidden="true"><circle cx="14" cy="5" r="4" class="feed" /></svg>
          {view.mixture.replace(/\.mix$/, "")}'s own composition
        </li>
      </ul>
      <p class="cursor muted" aria-live="polite">
        {#if cursor}
          {d.components.map((c, i) => `${c} ${formatNumber(cursor?.[i] ?? NaN)}`).join(" · ")}
        {:else}
          Mole fractions; point at the triangle to read them.
        {/if}
      </p>
    </div>
  {:else}
    <p class="muted">Classifying the feeds…</p>
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
    min-width: 160px;
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

  svg {
    display: block;
    max-width: 100%;
    font-size: 13px;
  }

  .region {
    fill: var(--ternary-region);
  }

  .grid {
    stroke: var(--border);
  }

  .side {
    fill: none;
    stroke: var(--border-strong);
    stroke-width: 1.5;
  }

  .tie {
    stroke: var(--fg-muted);
    stroke-width: 1;
  }

  .bubble,
  .dew {
    fill: none;
    stroke-width: 2.5;
  }

  .bubble {
    stroke: var(--iso-1);
  }

  .dew {
    stroke: var(--iso-2);
    stroke-dasharray: 6 4;
  }

  .feed {
    fill: var(--fg);
    stroke: var(--surface);
    stroke-width: 2;
  }

  .corner {
    fill: var(--fg);
    font-weight: 600;
  }

  .below {
    display: flex;
    flex-wrap: wrap;
    justify-content: space-between;
    gap: 12px;
    margin-top: 8px;
  }

  .legend {
    list-style: none;
    margin: 0;
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

  .swatch {
    width: 28px;
    height: 14px;
    border-radius: 4px;
    background: var(--ternary-region);
    border: 1px solid var(--border-strong);
  }

  .cursor {
    font-size: 13px;
    font-variant-numeric: tabular-nums;
  }

  .refusal strong {
    color: var(--danger);
  }
</style>

<script lang="ts">
  // A chart of paths the caller scaled (src/curves/chart.ts): its frame, ticks and titles, the paths clipped to the
  // frame, a label at the end of each line that has one, and its dots.
  import type { Chart } from "../curves/chart";

  let { chart, x, y, label, id }: { chart: Chart; x: string; y: string; label: string; id: string } = $props();
  const f = $derived(chart.frame);
</script>

<svg
  width={f.width}
  height={f.height}
  viewBox="0 0 {f.width} {f.height}"
  role="img"
  aria-label={label}
  data-testid={id}
  data-paths={chart.paths.length}
>
  <defs>
    <clipPath id="{id}-clip">
      <rect x={f.left} y={f.top} width={f.right - f.left} height={f.bottom - f.top} />
    </clipPath>
  </defs>
  <rect class="frame" x={f.left} y={f.top} width={f.right - f.left} height={f.bottom - f.top} />
  {#each chart.xTicks as t (t.at)}
    <text class="tick" x={t.at} y={f.bottom + 18} text-anchor="middle">{t.text}</text>
  {/each}
  {#each chart.yTicks as t (t.at)}
    <text class="tick" x={f.left - 8} y={t.at + 4} text-anchor="end">{t.text}</text>
  {/each}
  <g clip-path="url(#{id}-clip)">
    {#each chart.paths as p, i (i)}
      <path d={p.d} fill="none" stroke={p.colour} stroke-width={p.width} stroke-dasharray={p.dash || undefined} />
    {/each}
    {#each chart.dots as d, i (i)}
      <circle class="dot" cx={d.x} cy={d.y} r="4" />
    {/each}
  </g>
  {#each chart.paths as p, i (i)}
    {#if p.label}
      <text class="end" x={p.label.x + 6} y={p.label.y + 4}>{p.label.text}</text>
    {/if}
  {/each}
  <text class="title" x={(f.left + f.right) / 2} y={f.height - 8} text-anchor="middle">{x}</text>
  <text class="title" transform="translate(16 {(f.top + f.bottom) / 2}) rotate(-90)" text-anchor="middle">{y}</text>
</svg>

<style>
  svg {
    display: block;
    max-width: 100%;
    font-size: 12px;
  }

  .frame {
    fill: none;
    stroke: var(--border-strong);
  }

  .tick,
  .end {
    fill: var(--fg-muted);
    font-variant-numeric: tabular-nums;
  }

  .end {
    font-size: 11px;
  }

  .title {
    fill: var(--fg);
  }

  .dot {
    fill: var(--fg);
    stroke: var(--surface);
    stroke-width: 2;
  }
</style>

<script lang="ts">
  // The benchmark (PLAN.md M21.3): N states of a chosen pair on each build this browser runs, timed in this browser,
  // the cells by status, and whether the two builds' values are equal bit for bit (user decision SL8). Its view and
  // wiring are src/bench/view.svelte.ts; each build runs in its own worker.
  import { PAIRS } from "../bench/grids";
  import { BenchView, SIZES } from "../bench/view.svelte";
  import { builds, getEngine } from "../engine/client";
  import { FLUIDS } from "../lib/fluids";
  import { formatNumber } from "../lib/format";
  import { route, writeParams } from "../lib/route.svelte";

  const view = new BenchView(builds().map((build) => ({ build, engine: getEngine(build) })));
  $effect(() => view.connect(route, (params) => writeParams("benchmark", params)));

  const run = $derived(view.run?.key === view.key() ? view.run : null);
  const STATUS: Record<number, string> = { 0: "Values", 1: "Extrapolated" };
  const count = (counts: [number, number][], code: number) => counts.find(([c]) => c === code)?.[1] ?? 0;
  const failed = (counts: [number, number][]) => counts.filter(([c]) => c > 1).reduce((sum, [, n]) => sum + n, 0);
  const grouped = (x: number) => Math.round(x).toLocaleString("en-GB");
</script>

<section class="intro">
  <h1>Benchmark</h1>
  <p class="muted">
    N states of one input pair, each asked for h, s, c<sub>p</sub> and w, timed here in your browser on each build of phasekit's
    WebAssembly package: the baseline, and the simd128 build that runs eight states at a time in SIMD lanes. The two builds
    should agree to the last bit.
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
    <legend>Inputs</legend>
    <div class="segmented">
      {#each PAIRS as p (p.id)}
        <label class:active={view.pair === p.id}>
          <input type="radio" name="bench-pair" checked={view.pair === p.id} onchange={() => (view.pair = p.id)} />
          {p.label}
        </label>
      {/each}
    </div>
  </fieldset>
  <fieldset class="field">
    <legend>States</legend>
    <div class="segmented">
      {#each SIZES as n (n)}
        <label class:active={view.n === n}>
          <input type="radio" name="bench-n" checked={view.n === n} onchange={() => (view.n = n)} />
          {grouped(n)}
        </label>
      {/each}
    </div>
  </fieldset>
  <button type="button" class="run" onclick={() => view.start()} disabled={view.running} data-testid="run">
    {view.running ? "Running…" : "Run"}
  </button>
</div>

<section class="card results" data-testid="bench" data-state={view.running ? "running" : run ? "done" : "idle"}>
  {#if run?.refusal}
    <div class="refusal" role="alert">
      <strong>The package refused this run.</strong>
      <p><code>{run.refusal}</code></p>
    </div>
  {:else if run}
    <table>
      <thead>
        <tr>
          <th scope="col">Build</th>
          <th scope="col" class="num">States a second</th>
          <th scope="col" class="num">Time a run</th>
          {#each [0, 1] as code (code)}
            <th scope="col" class="num">{STATUS[code]}</th>
          {/each}
          <th scope="col" class="num">Refused</th>
        </tr>
      </thead>
      <tbody>
        {#each run.results as r (r.build)}
          <tr data-testid="bench-{r.build}">
            <th scope="row">{r.build}</th>
            <td class="num">{grouped(r.perSecond)}</td>
            <td class="num">{formatNumber(Number((r.timing.us / 1000).toPrecision(3)))} ms × {r.timing.runs}</td>
            <td class="num">{grouped(count(r.counts, 0))}</td>
            <td class="num">{grouped(count(r.counts, 1))}</td>
            <td class="num">{grouped(failed(r.counts))}</td>
          </tr>
        {/each}
      </tbody>
    </table>
    {#if run.comparison}
      {@const c = run.comparison}
      <p class="verdict" class:differ={c.differing > 0} data-testid="comparison" data-differing={c.differing}>
        {#if c.differing === 0}
          The two builds agree bit for bit on all {grouped(c.cells)} cells.
        {:else}
          The builds differ in {grouped(c.differing)} of {grouped(c.cells)} cells; the first is cell {c.first?.cell}:
          {c.first?.a} against {c.first?.b}.
        {/if}
      </p>
    {:else if !view.running && run.results.length === 1}
      <p class="muted">This browser runs no WebAssembly SIMD, so only the baseline build ran.</p>
    {/if}
  {:else}
    <p class="muted">Choose a fluid, an input pair and a number of states, then run.</p>
  {/if}
</section>

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

  .run {
    background: var(--accent);
    color: var(--on-accent);
    border: 0;
    border-radius: var(--radius-sm);
    padding: 8px 22px;
    font-weight: 600;
    cursor: pointer;
  }

  .run:disabled {
    opacity: 0.6;
    cursor: default;
  }

  .results {
    padding: 16px 18px;
  }

  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 14px;
  }

  th,
  td {
    padding: 6px 8px;
    border-bottom: 1px solid var(--border);
    text-align: left;
  }

  thead th {
    font-size: 12px;
    font-weight: 600;
    color: var(--fg-muted);
  }

  .num {
    text-align: right;
    font-variant-numeric: tabular-nums;
  }

  .verdict {
    margin: 14px 0 0;
    font-weight: 550;
  }

  .verdict.differ {
    color: var(--danger);
  }

  .refusal {
    padding: 16px 18px;
    border: 1px solid var(--danger);
    border-radius: var(--radius-sm);
  }

  .refusal strong {
    color: var(--danger);
  }
</style>

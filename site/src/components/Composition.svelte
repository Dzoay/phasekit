<script lang="ts">
  // A mixture's composition at a state (PLAN.md M21.5): each component's mole fraction in the feed and, at a two-phase
  // state, in the liquid and the vapour, every value the package's. A dash where there is none, so the table keeps its
  // size as the state changes. `compact`, for a narrow panel: no feed column, and the package's refusal of the phases
  // on the dashes' tooltip rather than on a line of its own, so the table's height never changes.
  import type { StateMixture } from "../engine/protocol";
  import { formatNumber } from "../lib/format";

  let {
    components,
    at,
    compact = false,
  }: { components: readonly string[]; at: StateMixture | null; compact?: boolean } = $props();

  const cell = (values: readonly number[] | null | undefined, i: number): string => {
    const v = values?.[i];
    return v === undefined ? "—" : formatNumber(v);
  };
  const refused = $derived(at?.error ? `The package refused the phases: ${at.error}` : undefined);
</script>

<table class="composition" class:compact data-testid="composition">
  <thead>
    <tr>
      <th scope="col">Mole fractions</th>
      {#if !compact}<th scope="col">Feed</th>{/if}
      <th scope="col">Liquid</th>
      <th scope="col">Vapour</th>
    </tr>
  </thead>
  <tbody>
    {#each components as name, i (i)}
      <tr>
        <th scope="row">{name}</th>
        {#if !compact}<td class="num" data-testid="feed-{i}">{cell(at?.feed, i)}</td>{/if}
        <td class="num" data-testid="liquid-{i}" data-value={at?.liquid?.[i]} title={refused}>{cell(at?.liquid, i)}</td>
        <td class="num" data-testid="vapour-{i}" data-value={at?.vapour?.[i]} title={refused}>{cell(at?.vapour, i)}</td>
      </tr>
    {/each}
  </tbody>
</table>
{#if at?.error && !compact}
  <p class="error" data-testid="phases-refused">
    The package refused the phases' compositions: <code>{at.error}</code>
  </p>
{/if}

<style>
  .composition {
    width: 100%;
    border-collapse: collapse;
    font-size: 13px;
    font-variant-numeric: tabular-nums;
  }

  th,
  td {
    padding: 3px 0 3px 10px;
    border-top: 1px solid var(--border);
    text-align: right;
    white-space: nowrap;
  }

  thead th {
    border-top: 0;
    font-weight: 500;
    color: var(--fg-muted);
  }

  th:first-child {
    padding-left: 0;
    text-align: left;
    font-weight: 450;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  thead th:first-child {
    font-weight: 500;
  }

  .compact {
    font-size: 12px;
  }

  .error {
    margin: 6px 0 0;
    font-size: 13px;
    color: var(--fg-muted);
  }
</style>

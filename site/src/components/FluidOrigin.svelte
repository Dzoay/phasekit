<script lang="ts">
  // Where the chosen fluid's data comes from, and what the page has fetched and the registry decoded (PLAN.md M21.4):
  // ARCHITECTURE.md §8's "only what is needed", made visible.
  import { fluidData } from "../engine/client";

  let { fluid }: { fluid: string } = $props();
</script>

<div class="origin" data-testid="fluid-data" aria-live="polite">
  {#if fluidData.error}<p class="error">{fluidData.error}</p>{/if}
  <p data-testid="fluid-origin">{fluidData.origin(fluid)}</p>
  <p data-testid="fluid-summary">{fluidData.summary}</p>
</div>

<style>
  /* Each line keeps its height while its text is on its way, so nothing below it moves. */
  .origin {
    display: grid;
    gap: 2px;
    font-size: 13px;
    line-height: 18px;
    color: var(--fg-muted);
  }

  p {
    margin: 0;
    min-height: 18px;
  }

  .error {
    color: var(--danger);
  }
</style>

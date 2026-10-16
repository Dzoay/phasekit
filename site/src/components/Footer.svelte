<script lang="ts">
  import { getEngine } from "../engine/client";

  const commit = __PHASEKIT_COMMIT__;
  let build = $state("loading…");

  $effect(() => {
    getEngine()
      .call({ op: "info" })
      .then(
        (info) => (build = info.build),
        () => (build = "unavailable"),
      );
  });
</script>

<footer>
  <div class="bar">
    <span>
      Built from
      {#if commit === "dev"}
        <code>dev</code>
      {:else}
        <a href="https://github.com/Dzoay/phasekit/commit/{commit}"><code>{commit.slice(0, 7)}</code></a>
      {/if}
    </span>
    <span>WebAssembly build <code data-testid="build">{build}</code></span>
    <span class="links">
      <a href="#/licences">Licences</a>
      <a href="https://github.com/Dzoay/phasekit">Source</a>
    </span>
  </div>
</footer>

<style>
  footer {
    border-top: 1px solid var(--border);
    margin-top: 48px;
    color: var(--fg-muted);
    font-size: 13px;
  }

  .bar {
    max-width: 1180px;
    margin: 0 auto;
    padding: 18px 16px 28px;
    display: flex;
    flex-wrap: wrap;
    gap: 8px 24px;
  }

  .links {
    margin-left: auto;
    display: flex;
    gap: 16px;
  }

  code {
    color: var(--fg);
  }
</style>

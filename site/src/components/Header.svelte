<script lang="ts">
  import { route } from "../lib/route.svelte";
  import ThemeControls from "./ThemeControls.svelte";

  const PAGES = [
    { id: "calculator", label: "Calculator" },
    { id: "diagrams", label: "Diagrams" },
  ];
</script>

<header>
  <div class="bar">
    <a class="brand" href="#/calculator">
      <svg viewBox="0 0 24 24" width="22" height="22" aria-hidden="true">
        <path d="M3 20 C 5 7, 9 4.5, 12 4.5 S 19 7, 21 20" fill="none" stroke="currentColor" stroke-width="2.2" />
        <circle cx="12" cy="4.5" r="2.4" fill="currentColor" />
        <path d="M3 20 H 21" stroke="currentColor" stroke-width="1.4" opacity="0.45" />
      </svg>
      <span>phasekit</span>
    </a>
    <nav aria-label="Pages">
      {#each PAGES as page (page.id)}
        <a href="#/{page.id}" aria-current={route.page === page.id ? "page" : undefined}>{page.label}</a>
      {/each}
    </nav>
    <ThemeControls />
  </div>
</header>

<style>
  header {
    position: sticky;
    top: 0;
    z-index: 10;
    background: color-mix(in srgb, var(--bg) 82%, transparent);
    backdrop-filter: saturate(1.4) blur(10px);
    border-bottom: 1px solid var(--border);
  }

  .bar {
    max-width: 1180px;
    margin: 0 auto;
    padding: 10px 16px;
    display: flex;
    align-items: center;
    gap: 24px;
  }

  .brand {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--fg);
    text-decoration: none;
    font-weight: 650;
    font-size: 17px;
    letter-spacing: -0.02em;
  }

  .brand svg {
    color: var(--accent);
  }

  nav {
    display: flex;
    gap: 4px;
    flex: 1;
  }

  nav a {
    color: var(--fg-muted);
    text-decoration: none;
    padding: 5px 10px;
    border-radius: var(--radius-sm);
    font-size: 14px;
    font-weight: 500;
  }

  nav a:hover {
    color: var(--fg);
  }

  nav a[aria-current="page"] {
    color: var(--fg);
    background: var(--surface-2);
  }

  @media (max-width: 560px) {
    .bar {
      gap: 12px;
      flex-wrap: wrap;
    }
  }
</style>

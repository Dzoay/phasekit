<script lang="ts">
  // The licences of everything the page ships, as scripts/site/build.sh collects them into notices/.
  const NOTICES = [
    { file: "LICENSE-MIT", title: "phasekit, under the MIT licence" },
    { file: "LICENSE-APACHE", title: "phasekit, under the Apache License 2.0" },
    { file: "LICENSE-THIRD-PARTY", title: "The fluid data: CoolProp's MIT notice" },
    { file: "THIRD-PARTY-CRATES.txt", title: "The Rust crates in the WebAssembly modules" },
    { file: "THIRD-PARTY-NPM.txt", title: "The npm packages in this page" },
    { file: "Inter-OFL-1.1.txt", title: "The Inter typeface, under the SIL Open Font License 1.1" },
  ];

  async function notice(file: string): Promise<string> {
    const response = await fetch(`./notices/${file}`);
    if (!response.ok) throw new Error(`${file}: HTTP ${response.status}`);
    return response.text();
  }
</script>

<section class="card licences">
  <h1>Licences</h1>
  <p class="muted">
    phasekit is free software under the MIT licence or the Apache License 2.0, at your option. Its fluid data comes from
    CoolProp under CoolProp's MIT licence. This page also ships the crates and packages below.
  </p>
  {#each NOTICES as n (n.file)}
    <details data-testid="notice-{n.file}">
      <summary>{n.title}</summary>
      {#await notice(n.file)}
        <p class="muted">Loading…</p>
      {:then text}
        <pre>{text}</pre>
      {:catch e}
        <p role="alert">{e instanceof Error ? e.message : String(e)}</p>
      {/await}
    </details>
  {/each}
</section>

<style>
  .licences {
    margin: 36px 0;
    padding: 22px 24px;
  }

  h1 {
    margin: 0 0 8px;
    font-size: 26px;
  }

  details {
    border-top: 1px solid var(--border);
    padding: 10px 0;
  }

  summary {
    cursor: pointer;
    font-weight: 550;
  }

  pre {
    white-space: pre-wrap;
    font-size: 12.5px;
    line-height: 1.45;
    background: var(--surface-2);
    border-radius: var(--radius-sm);
    padding: 12px 14px;
    max-height: 420px;
    overflow: auto;
  }
</style>

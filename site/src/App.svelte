<script lang="ts">
  import Footer from "./components/Footer.svelte";
  import Header from "./components/Header.svelte";
  import { route } from "./lib/route.svelte";
  import Calculator from "./pages/Calculator.svelte";
  import Licences from "./pages/Licences.svelte";
</script>

<Header />
<main>
  {#if route.page === "calculator"}
    <Calculator />
  {:else if route.page === "diagrams"}
    <!-- Loaded on first visit: the diagrams and d3 stay out of the calculator's first load. -->
    {#await import("./pages/Diagrams.svelte") then page}
      <page.default />
    {/await}
  {:else if route.page === "licences"}
    <Licences />
  {:else}
    <section class="card missing">
      <h1>No such page</h1>
      <p>There is no page <code>{route.page}</code> here. <a href="#/calculator">Open the calculator</a>.</p>
    </section>
  {/if}
</main>
<Footer />

<style>
  main {
    max-width: 1180px;
    margin: 0 auto;
    padding: 0 16px;
  }

  .missing {
    margin: 36px 0;
    padding: 22px 24px;
  }
</style>

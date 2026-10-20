<script lang="ts">
  // The mixture editor (PLAN.md M21.5): a mixture's components, each a fluid the library pairs with the others, and
  // their mole fractions as typed; a pure fluid mixed with another; a predefined mixture's fractions, the package's, to
  // change. Its view is src/lib/mixture.svelte.ts; the package checks that the fractions sum to one.
  import { fluidData } from "../engine/client";
  import { invalid } from "../lib/calculator.svelte";
  import { formatNumber } from "../lib/format";
  import { MixtureEditor } from "../lib/mixture.svelte";

  let {
    id,
    value = $bindable(),
    known = null,
  }: {
    id: string;
    value: string;
    /** The package's components and mole fractions of the fluid, where it is a mixture. */
    known?: { components: string[]; feed: number[] } | null;
  } = $props();

  const editor = new MixtureEditor(fluidData, { get: () => value, set: (name) => (value = name) });
  const joiners = $derived(editor.options());
</script>

<div class="mixture" data-testid="mixture">
  {#if editor.parts}
    <table>
      <thead>
        <tr><th scope="col">Component</th><th scope="col">Mole fraction</th><th></th></tr>
      </thead>
      <tbody>
        {#each editor.parts as part, i (i)}
          {@const options = editor.options(i)}
          {@const text = editor.fraction(i)}
          <tr>
            <td>
              <select
                id="{id}-component-{i}"
                aria-label="Component {i + 1}"
                value={part.name}
                onchange={(e) => editor.rename(i, e.currentTarget.value)}
              >
                {#if !options.some((o) => o.name === part.name)}<option value={part.name}>{part.name}</option>{/if}
                {#each options as o (o.name)}<option value={o.name}>{o.name}</option>{/each}
              </select>
            </td>
            <td>
              <input
                class="num"
                aria-label="Mole fraction of component {i + 1}"
                inputmode="decimal"
                autocomplete="off"
                spellcheck="false"
                value={text}
                aria-invalid={invalid(text)}
                oninput={(e) => editor.type(i, e.currentTarget.value)}
              />
            </td>
            <td>
              <button type="button" aria-label="Remove component {i + 1}" onclick={() => editor.remove(i)}>×</button>
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
    <p class="hint">Mole fractions, which the package checks sum to one.</p>
  {:else if known && known.components.length > 1}
    <p class="known" data-testid="known-composition">
      {known.components.map((c, i) => `${c} ${formatNumber(known.feed[i] ?? Number.NaN)}`).join(", ")}
    </p>
    <button type="button" class="edit" onclick={() => editor.edit(known.components, known.feed)}>
      Change the fractions
    </button>
  {/if}
  {#if joiners.length > 0}
    <select
      id="{id}-add"
      aria-label={editor.parts ? "Add a component" : "Make a mixture with"}
      value=""
      onchange={(e) => {
        const name = e.currentTarget.value;
        e.currentTarget.value = "";
        if (name) editor.add(name);
      }}
    >
      <option value="">{editor.parts ? "Add a component…" : "Mix with…"}</option>
      {#each joiners as o (o.name)}<option value={o.name}>{o.name}</option>{/each}
    </select>
  {/if}
</div>

<style>
  .mixture {
    display: grid;
    gap: 6px;
    font-size: 13px;
  }

  table {
    width: 100%;
    border-collapse: collapse;
  }

  th {
    text-align: left;
    font-weight: 500;
    color: var(--fg-muted);
    padding: 0 4px 2px 0;
  }

  td {
    padding: 2px 4px 2px 0;
  }

  td:nth-child(2) {
    width: 42%;
  }

  td:last-child {
    width: 1%;
  }

  select,
  input {
    width: 100%;
    min-width: 0;
    background: var(--surface);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-sm);
    padding: 4px 6px;
    font-size: 13px;
  }

  input[aria-invalid="true"] {
    border-color: var(--danger);
  }

  button {
    background: var(--surface-2);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    padding: 3px 8px;
    cursor: pointer;
  }

  button:hover {
    border-color: var(--accent);
    color: var(--accent);
  }

  .edit {
    justify-self: start;
  }

  .hint,
  .known {
    margin: 0;
    color: var(--fg-muted);
  }
</style>

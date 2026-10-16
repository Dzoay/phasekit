<script lang="ts">
  // The fluid picker (PLAN.md M21.4): a combobox over every fluid, searched by name, alias or CAS number. Its view is
  // src/lib/picker.svelte.ts; the input keeps the focus, and the highlighted option is its active descendant.
  import { fluidData } from "../engine/client";
  import { Picker } from "../lib/picker.svelte";

  let { id, value = $bindable() }: { id: string; value: string } = $props();

  const picker = new Picker(fluidData, { get: () => value, set: (name) => (value = name) });

  // The index and what the registry serves, asked for once the picker is on the page.
  $effect(() => {
    fluidData.index().catch(() => {});
    fluidData.touch();
  });

  // The highlighted option stays in view as the arrows move it.
  $effect(() => {
    if (picker.open) document.getElementById(`${id}-option-${picker.active}`)?.scrollIntoView({ block: "nearest" });
  });
</script>

<div class="picker">
  <input
    {id}
    role="combobox"
    aria-expanded={picker.open}
    aria-controls="{id}-list"
    aria-autocomplete="list"
    aria-activedescendant={picker.open && picker.hits.length > 0 ? `${id}-option-${picker.active}` : undefined}
    autocomplete="off"
    spellcheck="false"
    placeholder="Name, alias or CAS number"
    value={picker.text}
    onfocus={(e) => {
      picker.show();
      e.currentTarget.select();
    }}
    onclick={() => picker.open || picker.show()}
    oninput={(e) => picker.type(e.currentTarget.value)}
    onkeydown={(e) => picker.key(e.key) && e.preventDefault()}
    onblur={() => picker.dismiss()}
  />
  {#if picker.open}
    <!-- The input keeps the focus (a press on the list would take it) and its keys move the highlight. -->
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <ul id="{id}-list" role="listbox" aria-label="Fluids" tabindex="-1" onmousedown={(e) => e.preventDefault()}>
      {#each picker.hits as hit, i (hit.entry.name)}
        <li
          id="{id}-option-{i}"
          role="option"
          aria-selected={i === picker.active}
          class:active={i === picker.active}
          onclick={() => picker.pick(hit.entry)}
          onmousemove={() => (picker.active = i)}
        >
          <span class="name">{hit.entry.name}</span>
          {#if hit.via}<span class="via">{hit.via}</span>{/if}
          <span class="cost">{fluidData.cost(hit.entry)}</span>
        </li>
      {:else}
        <li class="none" role="option" aria-selected="false" aria-disabled="true">No fluid has that name</li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .picker {
    position: relative;
    min-width: 14em;
  }

  /* A page's row of controls sets --picker-padding and --picker-height to those of its selects. */
  input {
    width: 100%;
    min-width: 0;
    height: var(--picker-height, auto);
    background: var(--surface);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-sm);
    padding: var(--picker-padding, 7px 10px);
  }

  input::placeholder {
    color: var(--fg-muted);
  }

  ul {
    position: absolute;
    z-index: 20;
    top: calc(100% + 4px);
    left: 0;
    right: 0;
    min-width: 18em;
    max-height: 20em;
    overflow-y: auto;
    margin: 0;
    padding: 4px;
    list-style: none;
    background: var(--surface);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-sm);
    box-shadow: var(--shadow);
  }

  li {
    display: flex;
    align-items: baseline;
    gap: 8px;
    padding: 5px 8px;
    border-radius: 6px;
    cursor: pointer;
  }

  li.active {
    background: var(--surface-2);
    box-shadow: inset 3px 0 0 var(--accent);
  }

  .via {
    font-size: 13px;
    color: var(--fg-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .cost {
    margin-left: auto;
    font-size: 12px;
    color: var(--fg-muted);
    white-space: nowrap;
    font-variant-numeric: tabular-nums;
  }

  .none {
    color: var(--fg-muted);
    cursor: default;
  }
</style>

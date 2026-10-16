<script lang="ts">
  // The calculator: a fluid, two inputs, and every output the package gives at that state, from the engine worker.
  // The view lives in the URL (`#/calculator?fluid=Water&a=P:101325&b=Q:0&units=engineering`, values in SI).
  import { untrack } from "svelte";
  import { getEngine } from "../engine/client";
  import type { StateValue } from "../engine/protocol";
  import { FLUIDS, PRESETS, type Preset } from "../lib/fluids";
  import { formatNumber } from "../lib/format";
  import { GROUPS, HEADLINE, INPUTS, OUTPUTS, output, type Output } from "../lib/outputs";
  import { route, writeParams } from "../lib/route.svelte";
  import { SYSTEMS, displayUnit, fromSI, toSI, type System, type Unit } from "../lib/units";

  const KEYS = OUTPUTS.map((o) => o.key);
  const DEFAULT = { fluid: "Water", a: ["P", 101325], b: ["Q", 0] } as const;

  function readInput(raw: string | null, fallback: readonly [string, number]): [string, number] {
    const [key = "", text = ""] = (raw ?? "").split(":");
    const value = Number(text);
    return INPUTS.includes(key) && text !== "" && Number.isFinite(value) ? [key, value] : [fallback[0], fallback[1]];
  }

  // The view: SI values; the input fields' text is separate, so typing is never reformatted under the cursor.
  let fluid = $state<string>(DEFAULT.fluid);
  let aKey = $state<string>(DEFAULT.a[0]);
  let aValue = $state<number>(DEFAULT.a[1]);
  let bKey = $state<string>(DEFAULT.b[0]);
  let bValue = $state<number>(DEFAULT.b[1]);
  let system = $state<System>("si");
  let aText = $state("");
  let bText = $state("");

  let result = $state<StateValue | null>(null);
  let refusal = $state<string | null>(null);
  let busy = $state(true);
  let sequence = 0;

  const unitOf = (key: string): Unit => displayUnit(output(key)?.kind ?? "dimensionless", system);
  const shown = (key: string, value: number): string => {
    const x = fromSI(value, unitOf(key));
    return Number.isFinite(x) ? String(Number(x.toPrecision(12))) : "";
  };

  function showInputs(): void {
    aText = shown(aKey, aValue);
    bText = shown(bKey, bValue);
  }

  // From the URL, on load and on every hash change. The route is its only dependency: the view it sets, and the input
  // text `showInputs` reads it into, are untracked, or each edit would re-run this and restore the URL's old view.
  $effect(() => {
    const params = route.params;
    untrack(() => {
      fluid = params.get("fluid") ?? DEFAULT.fluid;
      [aKey, aValue] = readInput(params.get("a"), DEFAULT.a);
      [bKey, bValue] = readInput(params.get("b"), DEFAULT.b);
      system = params.get("units") === "engineering" ? "engineering" : "si";
      showInputs();
    });
  });

  // To the URL.
  $effect(() => {
    const params = new URLSearchParams({ fluid, a: `${aKey}:${aValue}`, b: `${bKey}:${bValue}` });
    if (system === "engineering") params.set("units", system);
    writeParams("calculator", params);
  });

  // The state, from the engine; a late answer to an older request is dropped.
  $effect(() => {
    const request = {
      op: "state" as const,
      fluid,
      name1: aKey,
      value1: aValue,
      name2: bKey,
      value2: bValue,
      outputs: KEYS,
    };
    const mine = ++sequence;
    busy = true;
    getEngine()
      .call(request)
      .then(
        (value) => {
          if (mine !== sequence) return;
          [result, refusal, busy] = [value, null, false];
        },
        (e: unknown) => {
          if (mine !== sequence) return;
          [result, refusal, busy] = [null, e instanceof Error ? e.message : String(e), false];
        },
      );
  });

  const values = $derived.by(() => {
    const map = new Map<string, number>();
    const errors = new Map<string, string>();
    if (result) {
      for (const [i, key] of KEYS.entries()) {
        const value = result.values[i];
        if (typeof value === "number" && !Number.isNaN(value)) map.set(key, value);
        else errors.set(key, result.errors[i] ?? "not defined at this state");
      }
    }
    return { map, errors };
  });

  function typed(which: "a" | "b", text: string): void {
    const key = which === "a" ? aKey : bKey;
    const x = Number(text.trim());
    if (which === "a") aText = text;
    else bText = text;
    if (text.trim() === "" || !Number.isFinite(x)) return;
    if (which === "a") aValue = toSI(x, unitOf(key));
    else bValue = toSI(x, unitOf(key));
  }

  const invalid = (text: string): boolean => text.trim() === "" || !Number.isFinite(Number(text.trim()));

  // A new input key keeps the state: its value is the current state's, where the package gave one.
  function rekey(which: "a" | "b", key: string): void {
    const value = values.map.get(key);
    if (which === "a") [aKey, aValue] = [key, value ?? aValue];
    else [bKey, bValue] = [key, value ?? bValue];
    showInputs();
  }

  function setSystem(next: System): void {
    system = next;
    showInputs();
  }

  function apply(preset: Preset): void {
    fluid = preset.fluid;
    [aKey, aValue] = preset.a;
    [bKey, bValue] = preset.b;
    showInputs();
  }

  const rows = $derived([
    { which: "a" as const, key: aKey, text: aText },
    { which: "b" as const, key: bKey, text: bText },
  ]);
  const label = (o: Output | undefined, key: string): string => (o ? `${o.label} (${o.symbol.replace("_", "")})` : key);
  const fluids = $derived(FLUIDS.some((f) => f.name === fluid) ? FLUIDS : [...FLUIDS, { name: fluid, label: fluid }]);
</script>

<section class="intro">
  <h1>Fluid properties, computed in your browser</h1>
  <p class="muted">
    Pick a fluid and any two of its state variables. phasekit's WebAssembly package computes every property at that
    state, here on your machine, with the equations of state CoolProp 8.0.0 uses and their published corrections.
  </p>
</section>

<div class="layout">
  <form class="card inputs" onsubmit={(e) => e.preventDefault()}>
    <div class="field">
      <label for="fluid">Fluid</label>
      <select id="fluid" bind:value={fluid}>
        {#each fluids as f (f.name)}
          <option value={f.name}>{f.label}</option>
        {/each}
      </select>
    </div>

    {#each rows as { which, key, text } (which)}
      <div class="field">
        <label for="key-{which}">{which === "a" ? "First input" : "Second input"}</label>
        <div class="input-row">
          <select
            id="key-{which}"
            aria-label="{which === 'a' ? 'First' : 'Second'} input variable"
            value={key}
            onchange={(e) => rekey(which, e.currentTarget.value)}
          >
            {#each INPUTS as k (k)}
              <option value={k}>{label(output(k), k)}</option>
            {/each}
          </select>
          <input
            id="value-{which}"
            aria-label="{which === 'a' ? 'First' : 'Second'} input value"
            class="num"
            inputmode="decimal"
            autocomplete="off"
            spellcheck="false"
            value={text}
            aria-invalid={invalid(text)}
            oninput={(e) => typed(which, e.currentTarget.value)}
          />
          <span class="unit">{unitOf(key).label}</span>
        </div>
      </div>
    {/each}

    <fieldset class="field systems">
      <legend>Units</legend>
      <div class="segmented">
        {#each SYSTEMS as s (s.id)}
          <label class:active={system === s.id}>
            <input type="radio" name="units" checked={system === s.id} onchange={() => setSystem(s.id)} />
            {s.label}
          </label>
        {/each}
      </div>
    </fieldset>

    <div class="field">
      <span class="caption">Try</span>
      <ul class="presets">
        {#each PRESETS as preset (preset.label)}
          <li><button type="button" onclick={() => apply(preset)}>{preset.label}</button></li>
        {/each}
      </ul>
    </div>
  </form>

  <div class="results" class:busy aria-live="polite">
    {#if refusal}
      <div class="card refusal" role="alert">
        <strong>The package refused this state.</strong>
        <p>{refusal}</p>
      </div>
    {:else if result}
      <div class="summary">
        <p class="phase">Phase <strong data-testid="phase">{result.phase}</strong></p>
        <div class="headline">
          {#each HEADLINE as key (key)}
            {@const o = output(key)}
            {@const v = values.map.get(key)}
            <div class="card stat">
              <span class="stat-label">{o?.label}</span>
              <span class="stat-value num" data-testid="headline-{key}">
                {v === undefined ? "—" : formatNumber(fromSI(v, unitOf(key)))}
                <small>{unitOf(key).label}</small>
              </span>
            </div>
          {/each}
        </div>
      </div>

      <div class="groups">
        {#each GROUPS as group (group.title)}
          {@const available = group.outputs.filter((o) => values.map.has(o.key))}
          {@const missing = group.outputs.filter((o) => !values.map.has(o.key))}
          <section class="card group">
            <h2>{group.title}</h2>
            {#if available.length > 0}
              <table>
                <tbody>
                  {#each available as o (o.key)}
                    {@const v = values.map.get(o.key) ?? Number.NaN}
                    {@const u = unitOf(o.key)}
                    {@const [base, sub] = o.symbol.split("_")}
                    <tr>
                      <td class="symbol"
                        >{base}{#if sub}<sub>{sub}</sub>{/if}</td
                      >
                      <th scope="row">{o.label}</th>
                      <td class="num value" data-testid="value-{o.key}" data-value={String(v)} title="{o.key} = {v}">
                        {formatNumber(fromSI(v, u))}
                      </td>
                      <td class="unit">{u.label}</td>
                    </tr>
                  {/each}
                </tbody>
              </table>
            {/if}
            {#if missing.length > 0}
              <details>
                <summary>{missing.length} not defined here</summary>
                <ul>
                  {#each missing as o (o.key)}
                    <li><code>{o.key}</code>: {values.errors.get(o.key)}</li>
                  {/each}
                </ul>
              </details>
            {/if}
          </section>
        {/each}
      </div>
    {:else}
      <div class="card loading">Starting the engine…</div>
    {/if}
  </div>
</div>

<style>
  .intro {
    max-width: 760px;
    margin: 36px 0 24px;
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

  .layout {
    display: grid;
    grid-template-columns: minmax(280px, 340px) 1fr;
    gap: 20px;
    align-items: start;
  }

  .inputs {
    position: sticky;
    top: 72px;
    padding: 18px;
    display: grid;
    gap: 16px;
  }

  .field {
    display: grid;
    gap: 6px;
    border: 0;
    margin: 0;
    padding: 0;
    min-width: 0;
  }

  .field > label,
  legend,
  .caption {
    font-size: 12px;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--fg-muted);
    padding: 0;
  }

  select,
  input:not([type="radio"]) {
    width: 100%;
    min-width: 0;
    background: var(--surface);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-sm);
    padding: 7px 10px;
  }

  input[aria-invalid="true"] {
    border-color: var(--danger);
  }

  .input-row {
    display: grid;
    grid-template-columns: 1fr;
    gap: 6px;
  }

  .input-row input {
    font-size: 17px;
  }

  .input-row {
    grid-template-areas: "key key" "value unit";
    grid-template-columns: 1fr auto;
    align-items: center;
  }

  .input-row select {
    grid-area: key;
  }

  .input-row .unit {
    min-width: 5.5em;
  }

  .unit {
    color: var(--fg-muted);
    font-size: 13px;
    white-space: nowrap;
  }

  .segmented {
    display: flex;
    background: var(--surface-2);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    padding: 3px;
  }

  .segmented label {
    position: relative;
    flex: 1;
    text-align: center;
    padding: 4px 8px;
    border-radius: 6px;
    font-size: 13px;
    color: var(--fg-muted);
    cursor: pointer;
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

  .presets {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .presets button {
    background: var(--surface-2);
    border: 1px solid var(--border);
    border-radius: 999px;
    padding: 3px 10px;
    font-size: 13px;
    cursor: pointer;
  }

  .presets button:hover {
    border-color: var(--accent);
    color: var(--accent);
  }

  .results {
    display: grid;
    gap: 20px;
    min-width: 0;
    transition: opacity 0.15s;
  }

  .results.busy {
    opacity: 0.7;
  }

  .phase {
    margin: 0 0 10px;
    color: var(--fg-muted);
    font-size: 13px;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    font-weight: 600;
  }

  .phase strong {
    margin-left: 6px;
    padding: 2px 9px;
    border-radius: 999px;
    background: var(--accent);
    color: var(--on-accent);
    text-transform: none;
    letter-spacing: 0;
    font-family: var(--mono);
    font-weight: 600;
  }

  .headline {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 10px;
  }

  .stat {
    padding: 12px 14px;
    display: grid;
    gap: 2px;
  }

  .stat-label {
    font-size: 12px;
    color: var(--fg-muted);
  }

  .stat-value {
    font-size: 21px;
    font-weight: 600;
    letter-spacing: -0.01em;
  }

  .stat-value small {
    font-size: 12px;
    font-weight: 500;
    color: var(--fg-muted);
    margin-left: 2px;
  }

  .groups {
    columns: 2 380px;
    column-gap: 20px;
  }

  .group {
    break-inside: avoid;
    margin: 0 0 20px;
    padding: 14px 16px 10px;
  }

  .group h2 {
    font-size: 14px;
    margin: 0 0 8px;
    font-weight: 650;
  }

  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 14px;
  }

  th,
  td {
    padding: 5px 0;
    border-top: 1px solid var(--border);
    vertical-align: baseline;
  }

  tr:first-child th,
  tr:first-child td {
    border-top: 0;
  }

  th {
    text-align: left;
    font-weight: 450;
  }

  .symbol {
    width: 4.6em;
    padding-right: 8px;
    color: var(--fg-muted);
    font-style: italic;
    white-space: nowrap;
  }

  .symbol sub {
    font-size: 0.75em;
  }

  .value {
    text-align: right;
    padding-right: 8px;
    white-space: nowrap;
  }

  td.unit {
    width: 1%;
  }

  details {
    margin-top: 6px;
    font-size: 13px;
    color: var(--fg-muted);
  }

  summary {
    cursor: pointer;
    padding: 4px 0;
  }

  details ul {
    margin: 4px 0 6px;
    padding-left: 18px;
  }

  .refusal {
    padding: 16px 18px;
    border-color: var(--danger);
  }

  .refusal strong {
    color: var(--danger);
  }

  .refusal p {
    margin: 6px 0 0;
    font-family: var(--mono);
    font-size: 13px;
  }

  .loading {
    padding: 24px;
    color: var(--fg-muted);
  }

  @media (max-width: 560px) {
    .headline {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
  }

  @media (max-width: 820px) {
    .layout {
      grid-template-columns: 1fr;
    }

    .inputs {
      position: static;
    }
  }
</style>

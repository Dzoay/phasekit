<script lang="ts">
  // The calculator: a fluid or a mixture, two inputs, and every output the package gives at that state, with a
  // mixture's composition and its phases' (PLAN.md M21.5). Its view and wiring are src/lib/calculator.svelte.ts; this
  // component only binds to them.
  import Composition from "../components/Composition.svelte";
  import FluidOrigin from "../components/FluidOrigin.svelte";
  import FluidPicker from "../components/FluidPicker.svelte";
  import MixtureEditor from "../components/MixtureEditor.svelte";
  import { fluidData, getEngine } from "../engine/client";
  import { Calculator, KEYS, invalid } from "../lib/calculator.svelte";
  import { PRESETS } from "../lib/fluids";
  import { formatNumber, formatTime } from "../lib/format";
  import { liquidLiquid } from "../lib/mixtures";
  import { remedyText } from "../lib/refusal";
  import { GROUPS, HEADLINE, INPUTS, output, type Output } from "../lib/outputs";
  import { route, writeParams } from "../lib/route.svelte";
  import { SYSTEMS, fromSI } from "../lib/units";

  const calc = new Calculator(getEngine());
  $effect(() => calc.connect(route, (params) => writeParams("calculator", params)));

  let build = $state("");
  $effect(() => {
    getEngine()
      .call({ op: "info" })
      .then(
        (info) => (build = info.build),
        () => {},
      );
  });

  const rows = $derived([
    { which: "a" as const, key: calc.aKey, text: calc.aText },
    { which: "b" as const, key: calc.bKey, text: calc.bText },
  ]);
  const label = (o: Output | undefined, key: string): string => (o ? `${o.label} (${o.symbol.replace("_", "")})` : key);
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
      <FluidPicker id="fluid" bind:value={calc.fluid} />
      <MixtureEditor id="mixture" bind:value={calc.fluid} known={calc.result?.mixture ?? null} />
      <FluidOrigin fluid={calc.fluid} />
    </div>

    {#each rows as { which, key, text } (which)}
      <div class="field">
        <label for="key-{which}">{which === "a" ? "First input" : "Second input"}</label>
        <div class="input-row">
          <select
            id="key-{which}"
            aria-label="{which === 'a' ? 'First' : 'Second'} input variable"
            value={key}
            onchange={(e) => calc.rekey(which, e.currentTarget.value)}
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
            oninput={(e) => calc.typed(which, e.currentTarget.value)}
          />
          <span class="unit">{calc.unitOf(key).label}</span>
        </div>
      </div>
    {/each}

    <fieldset class="field systems">
      <legend>Units</legend>
      <div class="segmented">
        {#each SYSTEMS as s (s.id)}
          <label class:active={calc.system === s.id}>
            <input type="radio" name="units" checked={calc.system === s.id} onchange={() => calc.setSystem(s.id)} />
            {s.label}
          </label>
        {/each}
      </div>
    </fieldset>

    <div class="field">
      <span class="caption">Try</span>
      <ul class="presets">
        {#each PRESETS as preset (preset.label)}
          <li><button type="button" onclick={() => calc.apply(preset)}>{preset.label}</button></li>
        {/each}
      </ul>
    </div>
  </form>

  <div class="results" class:busy={calc.busy} aria-live="polite">
    {#if calc.refusal}
      <div class="card refusal" role="alert">
        <strong>The package refused this state.</strong>
        <p class="message" data-testid="refusal-message"><code>{calc.refusalText}</code></p>
        {#each (calc.refusal.remedies ?? []).map(remedyText).filter((t) => t !== null) as text (text)}
          <p data-testid="remedy">{text}</p>
        {/each}
        {#if calc.refusal.nearest?.length}
          <p data-testid="did-you-mean">
            Did you mean
            {#each calc.refusal.nearest as name, i (name)}
              {#if i > 0},{/if}
              <button type="button" class="link" onclick={() => calc.pick(name)}>{name}</button>
            {/each}?
          </p>
        {/if}
        {#if fluidData.parts.has(calc.fluid) && liquidLiquid(calc.refusal.message)}
          <p data-testid="liquid-liquid">
            A mixture takes every input pair, so this is its refusal of two liquid phases, or of three phases: phasekit
            computes the equilibrium of a vapour and a liquid only.
          </p>
        {/if}
      </div>
    {:else if calc.result}
      <div class="summary">
        <p class="phase">Phase <strong data-testid="phase">{calc.result.phase}</strong></p>
        <div class="headline">
          {#each HEADLINE as key (key)}
            {@const o = output(key)}
            {@const v = calc.values.map.get(key)}
            <div class="card stat">
              <span class="stat-label">{o?.label}</span>
              <span class="stat-value num" data-testid="headline-{key}">
                {v === undefined ? "—" : formatNumber(fromSI(v, calc.unitOf(key)))}
                <small>{calc.unitOf(key).label}</small>
              </span>
            </div>
          {/each}
        </div>
        {#if calc.timing}
          <p class="timing" data-testid="timing">
            Computed here in your browser by the {build} build: one flash {formatTime(calc.timing.flash.us)}, all
            {KEYS.length} outputs {formatTime(calc.timing.outputs.us)}
            <span class="muted">(means of {calc.timing.flash.runs} and {calc.timing.outputs.runs} runs)</span>
          </p>
        {/if}
      </div>

      {#if calc.result.mixture}
        <section class="card group composition" data-testid="mixture-composition">
          <h2>Composition</h2>
          <Composition components={calc.result.mixture.components} at={calc.result.mixture} />
        </section>
      {/if}

      <div class="groups">
        {#each GROUPS as group (group.title)}
          {@const available = group.outputs.filter((o) => calc.values.map.has(o.key))}
          {@const missing = group.outputs.filter((o) => !calc.values.map.has(o.key))}
          <section class="card group">
            <h2>{group.title}</h2>
            {#if available.length > 0}
              <table>
                <tbody>
                  {#each available as o (o.key)}
                    {@const v = calc.values.map.get(o.key) ?? Number.NaN}
                    {@const u = calc.unitOf(o.key)}
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
                    <li><code>{o.key}</code>: {calc.values.errors.get(o.key)}</li>
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

  .timing {
    margin: 10px 0 0;
    font-size: 13px;
    color: var(--fg-muted);
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

  .composition {
    margin: 0;
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
  }

  .refusal .message code {
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

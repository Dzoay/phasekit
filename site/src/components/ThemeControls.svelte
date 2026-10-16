<script lang="ts">
  import {
    ACCENTS,
    ACCENT_LABELS,
    MODES,
    MODE_LABELS,
    applyTheme,
    loadTheme,
    saveTheme,
    type Theme,
  } from "../lib/theme";

  const storage = () => window.localStorage;
  const theme: Theme = $state(loadTheme(storage));

  $effect(() => {
    applyTheme(theme, document.documentElement);
    saveTheme(theme, storage);
  });
</script>

<div class="theme">
  <fieldset class="modes">
    <legend class="visually-hidden">Theme</legend>
    {#each MODES as mode (mode)}
      <label class:active={theme.mode === mode}>
        <input type="radio" name="theme-mode" value={mode} bind:group={theme.mode} />
        <span>{MODE_LABELS[mode]}</span>
      </label>
    {/each}
  </fieldset>
  <fieldset class="accents">
    <legend class="visually-hidden">Accent</legend>
    {#each ACCENTS as accent (accent)}
      <label class="swatch" data-accent={accent} title={ACCENT_LABELS[accent]}>
        <input type="radio" name="theme-accent" value={accent} bind:group={theme.accent} />
        <span class="visually-hidden">{ACCENT_LABELS[accent]}</span>
      </label>
    {/each}
  </fieldset>
</div>

<style>
  .theme {
    display: flex;
    align-items: center;
    gap: 14px;
  }

  fieldset {
    border: 0;
    margin: 0;
    padding: 0;
    display: flex;
  }

  .modes {
    background: var(--surface-2);
    border: 1px solid var(--border);
    border-radius: 999px;
    padding: 3px;
  }

  .modes label {
    position: relative;
    padding: 3px 11px;
    border-radius: 999px;
    font-size: 13px;
    color: var(--fg-muted);
    cursor: pointer;
    user-select: none;
  }

  .modes label.active {
    background: var(--surface);
    color: var(--fg);
    box-shadow: var(--shadow);
  }

  .modes input,
  .swatch input {
    position: absolute;
    inset: 0;
    opacity: 0;
    margin: 0;
    cursor: pointer;
  }

  .modes label:has(:focus-visible),
  .swatch:has(:focus-visible) {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }

  .accents {
    gap: 7px;
  }

  .swatch {
    position: relative;
    width: 18px;
    height: 18px;
    border-radius: 50%;
    background: var(--accent);
    cursor: pointer;
    box-shadow: 0 0 0 1px var(--border);
  }

  .swatch:has(:checked) {
    box-shadow:
      0 0 0 2px var(--surface),
      0 0 0 4px var(--accent);
  }

  @media (max-width: 560px) {
    .modes label {
      padding: 3px 8px;
    }
  }
</style>

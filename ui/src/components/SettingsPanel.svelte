<!-- Copyright 2026 The justRead Authors. SPDX-License-Identifier: Apache-2.0 -->
<script lang="ts">
  import { settings, saveSettings } from "../lib/settings/store.svelte";
  import { THEMES, type FontChoice, type ThemeId } from "../lib/settings/reader-settings";

  // Every control writes straight into the shared settings object; the
  // reader reacts to the change and re-lays out in place.
  const fonts: { id: FontChoice; label: string }[] = [
    { id: "publisher", label: "Book default" },
    { id: "serif", label: "Serif" },
    { id: "sans", label: "Sans-serif" },
    { id: "humanist", label: "Humanist" },
    { id: "mono", label: "Monospace" },
    { id: "custom", label: "Installed font…" },
  ];
  const themes: { id: ThemeId; label: string }[] = [
    { id: "light", label: "Light" },
    { id: "sepia", label: "Sepia" },
    { id: "dark", label: "Dark" },
    { id: "black", label: "Black" },
    { id: "custom", label: "Custom" },
  ];
  const swatch = (id: ThemeId) =>
    id === "custom" ? settings.customColors : id in THEMES ? THEMES[id as keyof typeof THEMES] : THEMES.light;
</script>

<section class="panel" aria-label="Reading settings">
  <h3>Layout</h3>
  <div class="seg" role="radiogroup" aria-label="Layout">
    <button role="radio" aria-checked={settings.flow === "paginated"} class:on={settings.flow === "paginated"}
      onclick={() => ((settings.flow = "paginated"), saveSettings())}>Pages</button>
    <button role="radio" aria-checked={settings.flow === "scrolled"} class:on={settings.flow === "scrolled"}
      onclick={() => ((settings.flow = "scrolled"), saveSettings())}>Scroll</button>
  </div>
  {#if settings.flow === "paginated"}
    <label class="row">Two-page spread
      <select bind:value={settings.spread} onchange={saveSettings}>
        <option value="auto">Automatic (wide screens)</option>
        <option value="always">Always</option>
        <option value="never">Never</option>
      </select>
    </label>
  {/if}

  <h3>Theme</h3>
  <div class="themes">
    {#each themes as t (t.id)}
      <button class="theme" class:on={settings.theme === t.id} title={t.label}
        style="background:{swatch(t.id).background};color:{swatch(t.id).text}"
        onclick={() => ((settings.theme = t.id), saveSettings())}>Aa</button>
    {/each}
  </div>
  {#if settings.theme === "custom"}
    <div class="colors">
      <label>Background <input type="color" bind:value={settings.customColors.background} onchange={saveSettings} /></label>
      <label>Text <input type="color" bind:value={settings.customColors.text} onchange={saveSettings} /></label>
      <label>Links <input type="color" bind:value={settings.customColors.link} onchange={saveSettings} /></label>
    </div>
  {/if}

  <h3>Text</h3>
  <label class="row">Font
    <select bind:value={settings.font} onchange={saveSettings}>
      {#each fonts as f (f.id)}<option value={f.id}>{f.label}</option>{/each}
    </select>
  </label>
  {#if settings.font === "custom"}
    <label class="row">Font name
      <input type="text" placeholder="e.g. Literata" bind:value={settings.customFont} onchange={saveSettings} />
    </label>
  {/if}
  <div class="row"><span>Size</span><span>{settings.fontSize}px</span></div>
  <input type="range" aria-label="Font size" min="12" max="36" step="1" bind:value={settings.fontSize} onchange={saveSettings} />
  <div class="row"><span>Line height</span><span>{settings.lineHeight.toFixed(2)}</span></div>
  <input type="range" aria-label="Line height" min="1" max="2.4" step="0.05" bind:value={settings.lineHeight} onchange={saveSettings} />
  <div class="row"><span>Margins</span><span>{settings.margin}px</span></div>
  <input type="range" aria-label="Margins" min="0" max="140" step="4" bind:value={settings.margin} onchange={saveSettings} />
  <label class="check"><input type="checkbox" bind:checked={settings.justify} onchange={saveSettings} /> Justify text</label>
  <label class="check"><input type="checkbox" bind:checked={settings.hyphenation} onchange={saveSettings} /> Hyphenation</label>
  <label class="check"><input type="checkbox" bind:checked={settings.publisherStyles} onchange={saveSettings} /> Keep publisher styles</label>
</section>

<style>
  .panel {
    padding: 4px 18px 24px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  h3 {
    margin: 14px 0 2px;
    font-size: 12px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--muted);
  }
  .row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 12px;
  }
  .row span + span {
    color: var(--muted);
  }
  .check {
    display: flex;
    gap: 8px;
    align-items: center;
  }
  .seg {
    display: grid;
    grid-template-columns: 1fr 1fr;
    border: 1px solid var(--border);
    border-radius: 8px;
    overflow: hidden;
  }
  .seg button {
    border: 0;
    background: none;
    padding: 7px;
    cursor: pointer;
  }
  .seg button.on {
    background: var(--accent);
    color: #fff;
  }
  .themes {
    display: flex;
    gap: 8px;
  }
  .theme {
    width: 44px;
    height: 44px;
    border-radius: 50%;
    border: 1px solid var(--border);
    cursor: pointer;
    font-family: Georgia, serif;
  }
  .theme.on {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
  .colors {
    display: flex;
    gap: 10px;
    flex-wrap: wrap;
  }
  .colors label {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  input[type="range"] {
    width: 100%;
    accent-color: var(--accent);
  }
</style>

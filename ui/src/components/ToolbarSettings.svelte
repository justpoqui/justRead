<!-- Copyright 2026 The justRead Authors. SPDX-License-Identifier: Apache-2.0 -->
<!-- Settings → Toolbar: which buttons the reader shows, their order, and where they sit. -->
<script lang="ts">
  import Icon from "./Icon.svelte";
  import { settings, saveSettings } from "../lib/settings/store.svelte";
  import { DEFAULT_TOOLBAR, TOOLBAR_ITEMS, type ToolbarItem } from "../lib/settings/reader-settings";

  // STEP 1: shown items first (in the user's order), then the hidden ones.
  const rows = $derived([
    ...settings.toolbar.map((id) => TOOLBAR_ITEMS.find((t) => t.id === id)!),
    ...TOOLBAR_ITEMS.filter((t) => !settings.toolbar.includes(t.id)),
  ]);

  // STEP 2: every change writes the whole list back (keeps it serializable).
  function toggle(id: ToolbarItem) {
    settings.toolbar = settings.toolbar.includes(id) ? settings.toolbar.filter((x) => x !== id) : [...settings.toolbar, id];
    saveSettings();
  }
  function move(id: ToolbarItem, by: -1 | 1) {
    const list = [...settings.toolbar];
    const i = list.indexOf(id);
    const j = i + by;
    if (i < 0 || j < 0 || j >= list.length) return;
    [list[i], list[j]] = [list[j]!, list[i]!];
    settings.toolbar = list;
    saveSettings();
  }
  function reset() {
    settings.toolbar = [...DEFAULT_TOOLBAR];
    settings.toolbarPosition = "bottom";
    saveSettings();
  }
</script>

<section class="tb" aria-label="Toolbar settings">
  <p class="hint">Back, the book title and ⚙ Settings are always on the top bar.</p>
  <div class="seg" role="radiogroup" aria-label="Toolbar position">
    {#each [["top", "Top bar"], ["bottom", "Bottom bar"]] as [id, label] (id)}
      <button role="radio" aria-checked={settings.toolbarPosition === id} class:on={settings.toolbarPosition === id}
        onclick={() => ((settings.toolbarPosition = id as "top" | "bottom"), saveSettings())}>{label}</button>
    {/each}
  </div>

  <ul>
    {#each rows as t (t.id)}
      {@const shown = settings.toolbar.includes(t.id)}
      {@const at = settings.toolbar.indexOf(t.id)}
      <li class:off={!shown}>
        <label><input type="checkbox" checked={shown} onchange={() => toggle(t.id)} /> {t.label}</label>
        {#if shown}
          <button class="icon-btn" title="Move {t.label} earlier" disabled={at === 0} onclick={() => move(t.id, -1)}><Icon name="up" size={18} /></button>
          <button class="icon-btn" title="Move {t.label} later" disabled={at === settings.toolbar.length - 1} onclick={() => move(t.id, 1)}><Icon name="down" size={18} /></button>
        {/if}
      </li>
    {/each}
  </ul>
  <button class="reset" onclick={reset}>Reset toolbar</button>
</section>

<style>
  .tb {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .hint {
    margin: 0;
    font-size: 12px;
    color: var(--muted);
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
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    border: 1px solid var(--border);
    border-radius: 8px;
  }
  li {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 4px 6px 4px 10px;
    min-height: 40px;
  }
  li + li {
    border-top: 1px solid var(--border);
  }
  li.off {
    color: var(--muted);
  }
  label {
    flex: 1;
    display: flex;
    gap: 8px;
    align-items: center;
  }
  .icon-btn:disabled {
    opacity: 0.3;
  }
  .reset {
    align-self: flex-start;
    border: 1px solid var(--border);
    background: none;
    border-radius: 8px;
    padding: 6px 12px;
    cursor: pointer;
  }
</style>

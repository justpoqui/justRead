<!-- Copyright 2026 The justRead Authors. SPDX-License-Identifier: Apache-2.0 -->
<!-- The full Settings screen (⚙ in the reader): everything changeable, by category. -->
<script lang="ts">
  import Icon from "./Icon.svelte";
  import SettingsPanel from "./SettingsPanel.svelte";
  import ToolbarSettings from "./ToolbarSettings.svelte";
  import { settings, saveSettings } from "../lib/settings/store.svelte";
  import { PROGRESS_DISPLAYS } from "../lib/settings/reader-settings";

  let { fixed = false, onclose }: { fixed?: boolean; onclose: () => void } = $props();

  // STEP 1: categories. PDFs/comics have page fitting instead of typography.
  type Cat = "layout" | "text" | "theme" | "toolbar" | "progress";
  const cats = $derived<{ id: Cat; label: string }[]>([
    { id: "layout", label: "Layout" },
    { id: "text", label: fixed ? "Page & zoom" : "Text" },
    { id: "theme", label: "Theme" },
    { id: "toolbar", label: "Toolbar" },
    { id: "progress", label: "Page indicator" },
  ]);
  let cat = $state<Cat>("layout");
  const label = $derived(cats.find((c) => c.id === cat)!.label);
</script>

<div class="screen" role="dialog" aria-modal="true" aria-label="Settings">
  <header>
    <h2>Settings</h2>
    <button class="icon-btn" title="Close settings" onclick={onclose}><Icon name="x" /></button>
  </header>
  <div class="body">
    <nav aria-label="Settings categories">
      {#each cats as c (c.id)}
        <button class:on={cat === c.id} aria-current={cat === c.id ? "page" : undefined} onclick={() => (cat = c.id)}>{c.label}</button>
      {/each}
    </nav>
    <section class="content" aria-label={label}>
      <h3>{label}</h3>
      {#if cat === "layout" || cat === "text" || cat === "theme"}
        <SettingsPanel {fixed} sections={[cat]} headings={false} />
      {:else if cat === "toolbar"}
        <ToolbarSettings />
      {:else}
        <p class="hint">What the indicator at the bottom shows. Tap it while reading to switch.</p>
        <div class="choices" role="radiogroup" aria-label="Page indicator">
          {#each PROGRESS_DISPLAYS as p (p.id)}
            <label><input type="radio" name="progress" checked={settings.progressDisplay === p.id}
              onchange={() => ((settings.progressDisplay = p.id), saveSettings())} /> {p.label}</label>
          {/each}
        </div>
        <p class="hint">
          {fixed ? "PDFs and comics have no chapters, so only whole-book views apply." : "In scroll layout there are no pages, so the percentages are used."}
          “Pages left in book” is an estimate for text books, since pages depend on font and screen size.
        </p>
      {/if}
    </section>
  </div>
</div>

<style>
  .screen {
    position: absolute;
    inset: 0;
    z-index: 40;
    display: flex;
    flex-direction: column;
    background: var(--bg);
  }
  header {
    display: flex;
    align-items: center;
    padding: 6px 10px 6px 18px;
    border-bottom: 1px solid var(--border);
  }
  h2 {
    flex: 1;
    font-size: 16px;
    margin: 0;
  }
  .body {
    flex: 1;
    min-height: 0;
    display: flex;
  }
  nav {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 12px 8px;
    width: 190px;
    border-right: 1px solid var(--border);
  }
  nav button {
    border: 0;
    background: none;
    text-align: left;
    padding: 9px 12px;
    border-radius: 8px;
    cursor: pointer;
  }
  nav button.on {
    background: var(--panel);
    color: var(--accent);
    font-weight: 600;
  }
  .content {
    flex: 1;
    overflow: auto;
    padding: 8px 18px 32px;
    max-width: 560px;
  }
  .content :global(.panel) {
    padding: 0;
  }
  h3 {
    font-size: 15px;
    margin: 12px 0 12px;
  }
  .hint {
    font-size: 12.5px;
    color: var(--muted);
  }
  .choices {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .choices label {
    display: flex;
    gap: 8px;
    align-items: center;
  }
  /* Phones: categories become a scrollable row of tabs above the content. */
  @media (max-width: 640px) {
    .body {
      flex-direction: column;
    }
    nav {
      flex-direction: row;
      width: auto;
      overflow-x: auto;
      border-right: 0;
      border-bottom: 1px solid var(--border);
      padding: 8px;
    }
    nav button {
      white-space: nowrap;
    }
  }
</style>

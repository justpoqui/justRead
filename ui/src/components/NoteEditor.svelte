<!-- Copyright 2026 The justRead Authors. SPDX-License-Identifier: Apache-2.0 -->
<script lang="ts">
  let {
    quote,
    initial = "",
    onsave,
    oncancel,
  }: { quote: string; initial?: string; onsave: (note: string) => void; oncancel: () => void } = $props();

  // The editor is created per note, so capturing the initial value is intended.
  // svelte-ignore state_referenced_locally
  let text = $state(initial);
  let area: HTMLTextAreaElement | undefined = $state();
  $effect(() => area?.focus());

  function key(e: KeyboardEvent) {
    e.stopPropagation(); // typing must not turn pages
    if (e.key === "Escape") oncancel();
    if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) onsave(text);
  }
</script>

<div class="backdrop" role="presentation" onclick={oncancel}>
  <div class="dialog" role="dialog" aria-modal="true" aria-label="Note" tabindex="-1" onclick={(e) => e.stopPropagation()} onkeydown={key}>
    <blockquote>{quote}</blockquote>
    <textarea bind:this={area} bind:value={text} rows="6" placeholder="Write a note…"></textarea>
    <div class="buttons">
      <button class="btn" onclick={oncancel}>Cancel</button>
      <button class="btn primary" onclick={() => onsave(text)}>Save</button>
    </div>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.35);
    display: grid;
    place-items: center;
    z-index: 60;
  }
  .dialog {
    width: min(520px, calc(100vw - 32px));
    background: var(--bg);
    border-radius: 14px;
    padding: 18px;
    display: flex;
    flex-direction: column;
    gap: 12px;
    box-shadow: 0 16px 48px rgba(0, 0, 0, 0.3);
  }
  blockquote {
    margin: 0;
    border-left: 3px solid var(--accent);
    padding-left: 10px;
    font-family: Georgia, serif;
    max-height: 120px;
    overflow: auto;
  }
  textarea {
    resize: vertical;
  }
  .buttons {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
</style>

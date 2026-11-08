<!-- Copyright 2026 The justRead Authors. SPDX-License-Identifier: Apache-2.0 -->
<!-- Floating menu shown above a text selection or an existing highlight. -->
<script lang="ts">
  import Icon from "./Icon.svelte";
  import { HIGHLIGHT_COLORS } from "../lib/reader/styles";

  let {
    rect,
    existing = false,
    color = null,
    oncolor,
    onnote,
    oncopy,
    ondelete,
  }: {
    rect: DOMRect;
    existing?: boolean;
    color?: string | null;
    oncolor: (c: string) => void;
    onnote: () => void;
    oncopy?: () => void;
    ondelete?: () => void;
  } = $props();

  // Mouse: above the selection when there is room, otherwise below.
  // Touch: the phone's own Copy/Share bar appears *above* the selection, so ours
  // goes below it (or well above if the selection is near the bottom of the screen).
  const touch = typeof matchMedia !== "undefined" && matchMedia("(pointer: coarse)").matches;
  const MENU_H = 52;
  const top = $derived(
    touch
      ? rect.bottom + 16 + MENU_H < window.innerHeight
        ? rect.bottom + 16
        : Math.max(8, rect.top - MENU_H - 72)
      : rect.top > 64
        ? rect.top - MENU_H
        : rect.bottom + 10,
  );
  const left = $derived(Math.max(8, Math.min(window.innerWidth - 300, rect.left + rect.width / 2 - 140)));
  const colors = [...Object.keys(HIGHLIGHT_COLORS), "underline"];
</script>

<div class="menu" style="top:{top}px;left:{left}px" role="toolbar" aria-label="Highlight">
  {#each colors as c (c)}
    <button
      class="swatch"
      class:on={color === c}
      class:underline={c === "underline"}
      style={c === "underline" ? "" : `background:${HIGHLIGHT_COLORS[c]}`}
      title={c === "underline" ? "Underline" : `Highlight ${c}`}
      onclick={() => oncolor(c)}>{c === "underline" ? "U" : ""}</button>
  {/each}
  <span class="sep"></span>
  <button class="icon-btn" title={existing ? "Edit note" : "Add note"} onclick={onnote}><Icon name="note" size={18} /></button>
  {#if oncopy}<button class="icon-btn" title="Copy" onclick={oncopy}><Icon name="copy" size={18} /></button>{/if}
  {#if existing && ondelete}<button class="icon-btn" title="Remove highlight" onclick={ondelete}><Icon name="trash" size={18} /></button>{/if}
</div>

<style>
  .menu {
    position: fixed;
    z-index: 50;
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 6px 8px;
    border-radius: 12px;
    background: var(--bg);
    border: 1px solid var(--border);
    box-shadow: 0 8px 28px rgba(0, 0, 0, 0.22);
  }
  .swatch {
    width: 24px;
    height: 24px;
    border-radius: 50%;
    border: 1px solid var(--border);
    cursor: pointer;
    padding: 0;
  }
  .swatch.on {
    outline: 2px solid var(--fg);
    outline-offset: 1px;
  }
  .swatch.underline {
    background: none;
    text-decoration: underline;
    font-weight: 700;
  }
  .sep {
    width: 1px;
    height: 22px;
    background: var(--border);
    margin: 0 4px;
  }
</style>

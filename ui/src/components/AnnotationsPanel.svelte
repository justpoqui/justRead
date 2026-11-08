<!-- Copyright 2026 The justRead Authors. SPDX-License-Identifier: Apache-2.0 -->
<script lang="ts">
  import Icon from "./Icon.svelte";
  import type { Annotation } from "../lib/ipc/types";
  import { HIGHLIGHT_COLORS } from "../lib/reader/styles";

  let {
    annotations,
    onselect,
    onedit,
    ondelete,
  }: {
    annotations: Annotation[];
    onselect: (a: Annotation) => void;
    onedit: (a: Annotation) => void;
    ondelete: (a: Annotation) => void;
  } = $props();

  let tab: "highlights" | "bookmarks" = $state("highlights");
  const shown = $derived(annotations.filter((a) => (tab === "bookmarks" ? a.kind === "bookmark" : a.kind !== "bookmark")));
</script>

<div class="tabs" role="tablist">
  <button role="tab" aria-selected={tab === "highlights"} class:on={tab === "highlights"} onclick={() => (tab = "highlights")}>
    Highlights & notes ({annotations.filter((a) => a.kind !== "bookmark").length})
  </button>
  <button role="tab" aria-selected={tab === "bookmarks"} class:on={tab === "bookmarks"} onclick={() => (tab = "bookmarks")}>
    Bookmarks ({annotations.filter((a) => a.kind === "bookmark").length})
  </button>
</div>

{#if shown.length === 0}
  <p class="empty">
    {tab === "bookmarks" ? "No bookmarks yet. Use the bookmark button in the toolbar." : "Select text in the book to highlight it or add a note."}
  </p>
{:else}
  <ul>
    {#each shown as a (a.id)}
      <li>
        <button class="item" onclick={() => onselect(a)}>
          {#if a.chapter}<span class="chapter">{a.chapter}</span>{/if}
          <span class="quote" style="border-color:{HIGHLIGHT_COLORS[a.color] ?? 'var(--accent)'}">{a.quote || "Bookmark"}</span>
          {#if a.note}<span class="note">{a.note}</span>{/if}
        </button>
        <div class="actions">
          {#if a.kind !== "bookmark"}
            <button class="icon-btn" title="Edit note" onclick={() => onedit(a)}><Icon name="note" size={16} /></button>
          {/if}
          <button class="icon-btn" title="Delete" onclick={() => ondelete(a)}><Icon name="trash" size={16} /></button>
        </div>
      </li>
    {/each}
  </ul>
{/if}

<style>
  .tabs {
    display: grid;
    grid-template-columns: 1fr 1fr;
    margin: 6px 12px 8px;
    border: 1px solid var(--border);
    border-radius: 8px;
    overflow: hidden;
  }
  .tabs button {
    border: 0;
    background: none;
    padding: 7px 4px;
    font-size: 12.5px;
    cursor: pointer;
  }
  .tabs button.on {
    background: var(--panel);
    font-weight: 600;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0 0 24px;
  }
  li {
    display: flex;
    align-items: flex-start;
    border-bottom: 1px solid var(--border);
  }
  .item {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 4px;
    text-align: left;
    border: 0;
    background: none;
    padding: 10px 6px 10px 14px;
    cursor: pointer;
    min-width: 0;
  }
  .chapter {
    font-size: 11.5px;
    color: var(--muted);
  }
  .quote {
    border-left: 3px solid;
    padding-left: 8px;
    font-family: Georgia, "Noto Serif", serif;
    line-height: 1.35;
    display: -webkit-box;
    -webkit-line-clamp: 4;
    line-clamp: 4;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  .note {
    font-size: 13px;
    color: var(--muted);
    white-space: pre-wrap;
  }
  .actions {
    display: flex;
    flex-direction: column;
    padding: 6px 6px 0 0;
  }
  .empty {
    color: var(--muted);
    padding: 12px 18px;
    line-height: 1.45;
  }
</style>

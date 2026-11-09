<!-- Copyright 2026 The justRead Authors. SPDX-License-Identifier: Apache-2.0 -->
<!-- Book options (library): shelf, collections, this book's reading stats, remove. -->
<script lang="ts">
  import { onMount } from "svelte";
  import Icon from "./Icon.svelte";
  import BookCover from "./BookCover.svelte";
  import { api } from "../lib/ipc/api";
  import type { BookStats, BookStatus, BookSummary, Collection } from "../lib/ipc/types";
  import { formatDuration } from "../lib/stats/format";
  import { toast } from "../lib/stores/toasts.svelte";
  import { onBack } from "../lib/back";

  let {
    book,
    collections,
    onchange,
    onremove,
    onclose,
  }: {
    book: BookSummary;
    collections: Collection[];
    /** Something changed: the library reloads books and collections. */
    onchange: () => void;
    onremove: () => void;
    onclose: () => void;
  } = $props();

  const SHELVES: { id: BookStatus; label: string }[] = [
    { id: "unread", label: "Unread" },
    { id: "reading", label: "Reading" },
    { id: "finished", label: "Finished" },
  ];
  let stats = $state<BookStats | null>(null);
  let newName = $state("");

  onMount(() => {
    api.bookStats(book.id).then((s) => (stats = s)).catch(() => {});
    return onBack(() => (onclose(), true));
  });

  async function run(task: () => Promise<unknown>) {
    try {
      await task();
      onchange();
    } catch (e) {
      toast(String(e), "error");
    }
  }
  const setShelf = (s: BookStatus) => run(() => api.setBookStatus(book.id, s));
  const toggle = (c: Collection, on: boolean) => run(() => api.setInCollection(book.id, c.id, on));
  function addCollection(e: SubmitEvent) {
    e.preventDefault();
    const name = newName.trim();
    if (!name) return;
    void run(async () => {
      const c = await api.createCollection(name);
      await api.setInCollection(book.id, c.id, true);
      newName = "";
    });
  }
</script>

<div class="backdrop" role="presentation" onclick={onclose}></div>
<div class="sheet" role="dialog" aria-modal="true" aria-label="Book options: {book.title}">
  <header>
    <div class="cover"><BookCover {book} small /></div>
    <div class="head">
      <h2>{book.title}</h2>
      <span class="muted">{book.authors.join(", ") || "Unknown author"} · {Math.round(book.progression * 100)}% read</span>
    </div>
    <button class="icon-btn" title="Close" onclick={onclose}><Icon name="x" /></button>
  </header>

  <h3>Shelf</h3>
  <div class="seg" role="radiogroup" aria-label="Shelf">
    {#each SHELVES as s (s.id)}
      <button role="radio" aria-checked={book.status === s.id} class:on={book.status === s.id} onclick={() => setShelf(s.id)}>{s.label}</button>
    {/each}
  </div>

  <h3>Collections</h3>
  {#if collections.length}
    <div class="colls">
      {#each collections as c (c.id)}
        <label><input type="checkbox" checked={book.collections.includes(c.id)} onchange={(e) => toggle(c, e.currentTarget.checked)} /> {c.name}</label>
      {/each}
    </div>
  {/if}
  <form class="new" onsubmit={addCollection}>
    <input type="text" placeholder="New collection…" aria-label="New collection name" maxlength="60" bind:value={newName} />
    <button type="submit" disabled={!newName.trim()}>Add</button>
  </form>

  <h3>Reading</h3>
  {#if stats && stats.seconds > 0}
    <dl class="stats">
      <div><dt>Time read</dt><dd>{formatDuration(stats.seconds)}</dd></div>
      <div><dt>Days read</dt><dd>{stats.daysRead}</dd></div>
      {#if stats.secondsLeft !== null}
        <div><dt>Time left at your pace</dt><dd>about {formatDuration(stats.secondsLeft)}</dd></div>
      {/if}
    </dl>
  {:else}
    <p class="muted">No reading time recorded yet.</p>
  {/if}

  <button class="remove" onclick={onremove}><Icon name="trash" size={16} /> Remove from library</button>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 40;
    background: rgba(0, 0, 0, 0.25);
  }
  .sheet {
    position: fixed;
    z-index: 41;
    top: 50%;
    left: 50%;
    transform: translate(-50%, -50%);
    width: min(440px, calc(100% - 24px));
    max-height: calc(100% - 48px);
    overflow: auto;
    padding: 14px 18px 18px;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 14px;
    box-shadow: 0 16px 48px rgba(0, 0, 0, 0.25);
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  header {
    display: flex;
    gap: 12px;
    align-items: flex-start;
  }
  .cover {
    width: 48px;
    flex: none;
  }
  .head {
    flex: 1;
    min-width: 0;
  }
  h2 {
    margin: 0 0 4px;
    font-size: 16px;
  }
  h3 {
    margin: 10px 0 0;
    font-size: 12px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--muted);
  }
  .muted {
    color: var(--muted);
    font-size: 13px;
  }
  .seg {
    display: grid;
    grid-template-columns: 1fr 1fr 1fr;
    border: 1px solid var(--border);
    border-radius: 8px;
    overflow: hidden;
  }
  .seg button {
    border: 0;
    background: none;
    padding: 8px;
    cursor: pointer;
  }
  .seg button.on {
    background: var(--accent);
    color: #fff;
  }
  .colls {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .colls label {
    display: flex;
    gap: 8px;
    align-items: center;
  }
  .new {
    display: flex;
    gap: 6px;
  }
  .new input {
    flex: 1;
    min-width: 0;
  }
  .new button,
  .remove {
    border: 1px solid var(--border);
    border-radius: 8px;
    background: none;
    padding: 6px 12px;
    cursor: pointer;
  }
  .stats {
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .stats div {
    display: flex;
    justify-content: space-between;
  }
  dt {
    color: var(--muted);
  }
  dd {
    margin: 0;
    font-variant-numeric: tabular-nums;
  }
  .remove {
    margin-top: 12px;
    align-self: flex-start;
    display: flex;
    align-items: center;
    gap: 6px;
    color: #c0392b;
  }
</style>

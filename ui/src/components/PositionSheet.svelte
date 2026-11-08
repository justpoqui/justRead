<!-- Copyright 2026 The justRead Authors. SPDX-License-Identifier: Apache-2.0 -->
<!--
  Go to position. Replaces the old always-visible slider, which was easy to
  bump: chapter steps and a fine slider inside the chapter move right away,
  while the whole-book slider only previews until "Go" is pressed.
-->
<script lang="ts">
  import Icon from "./Icon.svelte";
  import type { Manifest } from "../lib/ipc/types";
  import type { RelocateEvent } from "../lib/reader/engine";

  let {
    manifest,
    where,
    fixed,
    canGoBack,
    onchapter,
    onchapterfraction,
    onbookfraction,
    onback,
    onclose,
  }: {
    manifest: Manifest;
    where: RelocateEvent;
    fixed: boolean;
    canGoBack: boolean;
    onchapter: (spine: number) => void;
    onchapterfraction: (f: number) => void;
    onbookfraction: (f: number) => void;
    onback: () => void;
    onclose: () => void;
  } = $props();

  const spine = $derived(manifest.spine);
  const chapterName = (i: number) => spine[i]?.title?.trim() || `Section ${i + 1}`;

  // STEP 1: fine slider inside the chapter (or the whole book for PDFs/comics).
  // It shows pages when there are pages, otherwise a percentage.
  const fine = $derived(fixed ? where.progression : (where.chapterFraction ?? 0));
  let fineDrag = $state<number | null>(null);
  const fineSteps = $derived(where.pages > 1 ? where.pages : 1000);
  const fineValue = $derived(fineDrag ?? (where.pages > 1 ? where.page - 1 : Math.round(fine * 1000)));
  const fineLabel = $derived(
    where.pages > 1 ? `Page ${fineValue + 1} of ${where.pages}` : `${Math.round((fineValue / 1000) * 100)}%`,
  );
  function commitFine(v: number) {
    fineDrag = null;
    onchapterfraction(where.pages > 1 ? v / Math.max(1, where.pages - 1) : v / 1000);
  }

  // STEP 2: whole-book slider = preview only; "Go" commits.
  let preview = $state<number | null>(null);
  const previewLabel = $derived.by(() => {
    if (preview === null) return "";
    if (fixed) return `Page ${Math.min(where.pages, Math.floor(preview * where.pages) + 1)}`;
    // Which chapter holds this fraction of the text?
    const total = spine.reduce((s, x) => s + x.chars, 0) || 1;
    let acc = 0;
    let i = 0;
    for (; i < spine.length - 1; i++) {
      if (preview * total < acc + spine[i]!.chars) break;
      acc += spine[i]!.chars;
    }
    return chapterName(i);
  });
</script>

<div class="sheet" role="dialog" aria-label="Go to position">
  <header>
    <h2>Position</h2>
    <button class="icon-btn" title="Close position" onclick={onclose}><Icon name="x" /></button>
  </header>

  {#if !fixed}
    <div class="chapter">
      <button class="icon-btn" title="Previous chapter" disabled={where.spine <= 0} onclick={() => onchapter(where.spine - 1)}><Icon name="left" /></button>
      <div class="ch-text">
        <span class="ch-name">{chapterName(where.spine)}</span>
        <span class="muted">Chapter {where.spine + 1} of {spine.length}</span>
      </div>
      <button class="icon-btn" title="Next chapter" disabled={where.spine >= spine.length - 1} onclick={() => onchapter(where.spine + 1)}><Icon name="right" /></button>
    </div>
  {/if}

  <div class="row"><span>{fixed ? "In this book" : "In this chapter"}</span><span class="muted fine-label">{fineLabel}</span></div>
  <input type="range" class="fine" aria-label={fixed ? "Page" : "Position in chapter"} min="0" max={fineSteps - (where.pages > 1 ? 1 : 0)} value={fineValue}
    oninput={(e) => (fineDrag = Number(e.currentTarget.value))}
    onchange={(e) => commitFine(Number(e.currentTarget.value))} />

  {#if !fixed}
    <div class="row"><span>Whole book</span><span class="muted">{Math.round((preview ?? where.progression) * 100)}%</span></div>
    <input type="range" class="book" aria-label="Position in book (preview)" min="0" max="1000" value={Math.round((preview ?? where.progression) * 1000)}
      oninput={(e) => (preview = Number(e.currentTarget.value) / 1000)} />
    {#if preview !== null}
      <div class="go">
        <span class="preview">→ {previewLabel}</span>
        <button class="secondary" onclick={() => (preview = null)}>Cancel</button>
        <button class="primary" onclick={() => { const f = preview!; preview = null; onbookfraction(f); }}>Go</button>
      </div>
    {/if}
  {/if}

  {#if canGoBack}
    <button class="back" onclick={onback}><Icon name="back" size={16} /> Back to where you were</button>
  {/if}
</div>

<style>
  .sheet {
    position: absolute;
    left: 50%;
    bottom: 8px;
    transform: translateX(-50%);
    width: min(480px, calc(100% - 16px));
    z-index: 25;
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 6px 16px 16px;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 14px;
    box-shadow: 0 10px 36px rgba(0, 0, 0, 0.22);
  }
  header {
    display: flex;
    align-items: center;
  }
  h2 {
    flex: 1;
    font-size: 15px;
    margin: 0;
  }
  .chapter {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px;
    border: 1px solid var(--border);
    border-radius: 10px;
  }
  .chapter .icon-btn:disabled {
    opacity: 0.3;
  }
  .ch-text {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
  }
  .ch-name {
    font-weight: 600;
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .muted {
    color: var(--muted);
    font-size: 12.5px;
    font-variant-numeric: tabular-nums;
  }
  .row {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    margin-top: 6px;
  }
  input[type="range"] {
    width: 100%;
    accent-color: var(--accent);
  }
  .go {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .preview {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .go button,
  .back {
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 6px 14px;
    background: none;
    cursor: pointer;
  }
  .go .primary {
    background: var(--accent);
    border-color: var(--accent);
    color: #fff;
  }
  .back {
    display: flex;
    align-items: center;
    gap: 6px;
    align-self: flex-start;
    margin-top: 4px;
  }
</style>

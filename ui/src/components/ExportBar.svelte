<!-- Copyright 2026 The justRead Authors. SPDX-License-Identifier: Apache-2.0 -->
<!--
  Export a book's highlights, notes and bookmarks as Markdown, to any app:
  Share opens the system share sheet (Android: our native plugin; iOS,
  Windows, macOS: the web Share API), Save writes a .md file through a
  system Save dialog, Copy puts the text on the clipboard.
-->
<script lang="ts">
  import Icon from "./Icon.svelte";
  import { api } from "../lib/ipc/api";
  import { toast } from "../lib/stores/toasts.svelte";

  let { bookId, count }: { bookId: string; count: number } = $props();

  // STEP 1: is there a share sheet on this device? (Linux desktops have none.)
  const android = typeof navigator !== "undefined" && /Android/i.test(navigator.userAgent);
  const canShare = android || (typeof navigator !== "undefined" && typeof navigator.share === "function");
  let busy = $state(false);

  async function run(task: () => Promise<void>) {
    if (busy) return;
    busy = true;
    try {
      await task();
    } catch (e) {
      // Closing the share sheet is not an error.
      if ((e as DOMException)?.name !== "AbortError") toast(String(e), "error");
    } finally {
      busy = false;
    }
  }

  // STEP 2: Share — the file where the platform takes files, else the text.
  const share = () =>
    run(async () => {
      const doc = await api.exportAnnotations(bookId);
      if (doc.nativeShare) return api.shareAnnotations(bookId);
      const file = new File([doc.markdown], doc.fileName, { type: "text/markdown" });
      if (navigator.canShare?.({ files: [file] })) await navigator.share({ files: [file], title: doc.title });
      else await navigator.share({ title: doc.title, text: doc.markdown });
    });

  // STEP 3: Save — the Save dialog is opened by the Rust side.
  const save = () =>
    run(async () => {
      const name = await api.saveAnnotations(bookId);
      if (name) toast(`Saved “${name}”`);
    });

  // STEP 4: Copy.
  const copy = () =>
    run(async () => {
      const doc = await api.exportAnnotations(bookId);
      await navigator.clipboard.writeText(doc.markdown);
      toast("Copied as Markdown");
    });
</script>

<div class="export" aria-label="Export highlights">
  <span class="label">Export</span>
  {#if canShare}
    <button disabled={busy || count === 0} title="Send highlights and notes to another app" onclick={share}><Icon name="share" size={16} /> Share…</button>
  {/if}
  <button disabled={busy || count === 0} title="Save highlights and notes as a Markdown file" onclick={save}><Icon name="save" size={16} /> Save…</button>
  <button disabled={busy || count === 0} title="Copy highlights and notes as Markdown" onclick={copy}><Icon name="copy" size={16} /> Copy</button>
</div>

<style>
  .export {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
    margin: 0 12px 8px;
  }
  .label {
    font-size: 11.5px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--muted);
    margin-right: auto;
  }
  button {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    border: 1px solid var(--border);
    background: none;
    border-radius: 8px;
    padding: 5px 10px;
    cursor: pointer;
    font-size: 13px;
  }
  button:disabled {
    opacity: 0.45;
    cursor: default;
  }
</style>

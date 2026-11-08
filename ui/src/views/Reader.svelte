<!-- Copyright 2026 The justRead Authors. SPDX-License-Identifier: Apache-2.0 -->
<script lang="ts">
  import { onMount } from "svelte";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import Icon from "../components/Icon.svelte";
  import SettingsPanel from "../components/SettingsPanel.svelte";
  import TocPanel from "../components/TocPanel.svelte";
  import AnnotationsPanel from "../components/AnnotationsPanel.svelte";
  import SelectionMenu from "../components/SelectionMenu.svelte";
  import NoteEditor from "../components/NoteEditor.svelte";
  import { api, packageUrl } from "../lib/ipc/api";
  import type { Annotation, OpenedBook } from "../lib/ipc/types";
  import { ReaderEngine, type Engine, type RelocateEvent, type SelectionEvent } from "../lib/reader/engine";
  import { formatLocator } from "../lib/reader/locator";
  import { bookmarksInRange, bookmarksOnPages } from "../lib/reader/bookmarks";
  import { FixedEngine } from "../lib/fixed/fixed-engine";
  import { ImageSource, PdfSource } from "../lib/fixed/sources";
  import type { TocEntry } from "../lib/ipc/types";
  import { settings, saveSettings, setLayout, currentLayout, LAYOUTS } from "../lib/settings/store.svelte";
  import { toast } from "../lib/stores/toasts.svelte";

  let { id, onclose }: { id: string; onclose: () => void } = $props();

  let host = $state<HTMLDivElement | undefined>();
  let book = $state<OpenedBook | null>(null);
  // $state.raw: the engine object itself is not made deeply reactive.
  let engine = $state.raw<Engine | null>(null);
  let toc = $state<TocEntry[]>([]);
  const fixed = $derived(book?.manifest.layout === "fixed");
  let where = $state<RelocateEvent | null>(null);
  let annotations = $state<Annotation[]>([]);
  let panel = $state<"toc" | "notes" | "settings" | null>(null);
  let chrome = $state(true);
  let selection = $state<SelectionEvent | null>(null);
  let active = $state<{ annotation: Annotation; rect: DOMRect } | null>(null);
  let editing = $state<{ quote: string; initial: string; save: (note: string) => Promise<void> } | null>(null);
  let scrub = $state<number | null>(null);
  let layoutMenu = $state(false);
  /** Touch screens (phones/tablets): the OS adds its own selection toolbar. */
  const touch = typeof matchMedia !== "undefined" && matchMedia("(pointer: coarse)").matches;
  const LAYOUT_ICONS = { single: "page", double: "pages", scroll: "scroll" } as const;

  const title = $derived(book?.book.title ?? "");
  const currentHref = $derived(
    where && book ? (fixed ? `page:${where.page - 1}` : book.manifest.spine[where.spine]?.href ?? null) : null,
  );
  const pageBookmarks = $derived(
    fixed ? bookmarksOnPages(annotations, where?.visiblePages ?? null) : bookmarksInRange(annotations, where?.range ?? null),
  );

  // STEP 1: open the book, then create the engine at the saved position.
  onMount(() => {
    let saveTimer: ReturnType<typeof setTimeout> | undefined;
    let disposed = false;

    (async () => {
      try {
        book = await api.openBook(id);
        annotations = await api.listAnnotations(id);
        void getCurrentWindow().setTitle(`${book.book.title} — justRead`);
        if (disposed || !host) return;

        // STEP 1b: reflowable books use the text engine, PDFs and comics the page engine.
        const m = book.manifest;
        const snap = $state.snapshot(settings);
        if (m.layout === "fixed" && m.fixed) {
          const source =
            m.fixed.kind === "pdf"
              ? await PdfSource.open(packageUrl(id, m.fixed.document!))
              : new ImageSource(m, (href) => packageUrl(id, href));
          if (disposed) return source.destroy();
          engine = new FixedEngine(host, m, source, snap);
        } else {
          engine = new ReaderEngine({
            host,
            manifest: m,
            settings: snap,
            loadChapter: (href) => api.readChapter(id, href),
            contentBase: packageUrl(id, "content/"),
          });
        }
        engine.setAnnotations($state.snapshot(annotations));

        // STEP 2: wire engine events to UI state.
        engine.on("relocate", (e) => {
          where = e;
          clearTimeout(saveTimer);
          // Debounced: saving on every page turn would be wasteful.
          saveTimer = setTimeout(() => void api.saveProgress(id, formatLocator(e.anchor), e.progression), 600);
        });
        engine.on("selection", (s) => {
          selection = s;
          active = null;
        });
        engine.on("highlight", ({ id: hid, rect }) => {
          const a = annotations.find((x) => x.id === hid);
          if (a) active = { annotation: a, rect };
          selection = null;
        });
        engine.on("externalLink", (url) => {
          void navigator.clipboard?.writeText(url).catch(() => {});
          toast(`External link copied: ${url}`);
        });
        engine.on("key", onKey);
        engine.on("center", () => (chrome = !chrome));
        engine.on("zoom", (f) => {
          settings.zoom = Math.min(5, Math.max(0.5, Math.round(settings.zoom * f * 100) / 100));
          saveSettings();
        });
        engine.on("error", (e) => toast(e.message, "error"));

        await engine.open(book.progress?.locator);
        toc = (await engine.toc()) ?? m.toc;
      } catch (e) {
        toast(String(e), "error", 8000);
        onclose();
      }
    })();

    return () => {
      disposed = true;
      clearTimeout(saveTimer);
      // Flush the final position immediately when leaving the book.
      if (where) void api.saveProgress(id, formatLocator(where.anchor), where.progression);
      engine?.destroy();
    };
  });

  // STEP 3: settings changes re-layout in place (or switch views) — position is kept.
  $effect(() => {
    const snap = $state.snapshot(settings);
    engine?.setSettings(snap);
  });

  // ----- keyboard ---------------------------------------------------------
  function onKey(e: KeyboardEvent) {
    if (editing) return;
    const t = e.target as HTMLElement | null;
    if (t && (t.tagName === "INPUT" || t.tagName === "TEXTAREA" || t.tagName === "SELECT")) return;
    // Right-to-left books (manga) mirror the arrow keys.
    const key = engine?.rtl && (e.key === "ArrowLeft" || e.key === "ArrowRight") ? (e.key === "ArrowLeft" ? "ArrowRight" : "ArrowLeft") : e.key;
    switch (key) {
      case "+":
      case "=":
        if (fixed) ((settings.zoom = Math.min(5, Math.round(settings.zoom * 110) / 100)), saveSettings());
        break;
      case "-":
        if (fixed) ((settings.zoom = Math.max(0.5, Math.round(settings.zoom * 91) / 100)), saveSettings());
        break;
      case "0":
        if (fixed) ((settings.zoom = 1), saveSettings());
        break;
      case "ArrowRight":
      case "PageDown":
        e.preventDefault();
        void engine?.next();
        break;
      case " ":
        e.preventDefault();
        void (e.shiftKey ? engine?.prev() : engine?.next());
        break;
      case "ArrowLeft":
      case "PageUp":
        e.preventDefault();
        void engine?.prev();
        break;
      case "Escape":
        if (layoutMenu) layoutMenu = false;
        else if (selection || active) {
          selection = null;
          active = null;
          engine?.clearSelection();
        } else if (panel) panel = null;
        else onclose();
        break;
      case "b":
        if (!e.ctrlKey && !e.metaKey) void toggleBookmark();
        break;
    }
  }

  // ----- annotations ------------------------------------------------------
  async function refreshAnnotations() {
    annotations = await api.listAnnotations(id);
    engine?.setAnnotations($state.snapshot(annotations));
  }

  async function createHighlight(color: string, note = ""): Promise<Annotation | null> {
    if (!selection || !book) return null;
    const s = selection;
    try {
      const a = await api.addAnnotation({
        bookId: id,
        kind: note ? "note" : "highlight",
        start: formatLocator(s.start),
        end: formatLocator(s.end),
        quote: s.text.trim(),
        note,
        color,
        chapter: book.manifest.spine[s.start.spine]?.title ?? null,
      });
      selection = null;
      engine?.clearSelection();
      await refreshAnnotations();
      return a;
    } catch (e) {
      toast(String(e), "error");
      return null;
    }
  }

  function noteForSelection() {
    if (!selection) return;
    const quote = selection.text.trim();
    editing = {
      quote,
      initial: "",
      save: async (note) => {
        await createHighlight("yellow", note);
        editing = null;
      },
    };
  }

  function editNote(a: Annotation) {
    active = null;
    editing = {
      quote: a.quote,
      initial: a.note,
      save: async (note) => {
        await api.updateAnnotation(a.id, { note });
        editing = null;
        await refreshAnnotations();
      },
    };
  }

  async function recolor(a: Annotation, color: string) {
    await api.updateAnnotation(a.id, { color });
    active = null;
    await refreshAnnotations();
  }

  async function removeAnnotation(a: Annotation) {
    await api.deleteAnnotation(a.id);
    active = null;
    await refreshAnnotations();
  }

  async function toggleBookmark() {
    if (!where || !book) return;
    if (fixed) {
      if (pageBookmarks.length) for (const b of pageBookmarks) await api.deleteAnnotation(b.id);
      else await api.addAnnotation({ bookId: id, kind: "bookmark", start: formatLocator(where.anchor), quote: `Page ${where.page}` });
      await refreshAnnotations();
      return;
    }
    if (pageBookmarks.length) {
      for (const b of pageBookmarks) await api.deleteAnnotation(b.id);
    } else {
      // A bookmark remembers the top of the page plus a snippet for the list.
      const snippet = await snippetAt();
      await api.addAnnotation({
        bookId: id,
        kind: "bookmark",
        start: formatLocator(where.range!.start),
        quote: snippet,
        chapter: where.chapterTitle,
      });
    }
    await refreshAnnotations();
  }

  async function snippetAt(): Promise<string> {
    // The first ~120 characters of the visible text.
    const r = where?.range;
    if (!r) return "";
    const frame = host?.querySelector("iframe");
    const doc = frame?.contentDocument;
    const block = doc?.querySelector(`[data-jrb="${r.start.block}"]`);
    return (block?.textContent ?? "").slice(r.start.offset, r.start.offset + 120).trim();
  }

  function jumpTo(a: Annotation) {
    panel = null;
    void engine?.goToLocator(a.start);
  }

  function copySelection() {
    if (selection) void navigator.clipboard?.writeText(selection.text);
    selection = null;
    engine?.clearSelection();
  }

  // "Pages 4–5 of 8" when a fixed-layout spread shows two pages.
  const pageLabel = $derived.by(() => {
    if (!where) return "";
    const v = where.visiblePages;
    if (fixed && settings.flow === "paginated" && v && v.length > 1) return `Pages ${v[0]! + 1}–${v[v.length - 1]! + 1} of ${where.pages}`;
    return `Page ${where.page} of ${where.pages}`;
  });
  const pct = $derived(Math.round((scrub ?? where?.progression ?? 0) * 100));
</script>

<svelte:window onkeydown={onKey} />

<div class="reader" class:hide-chrome={!chrome && !panel}>
  <header class="bar top">
    <button class="icon-btn" title="Back to library (Esc)" onclick={onclose}><Icon name="back" /></button>
    <div class="titles">
      <span class="book">{title}</span>
      {#if where?.chapterTitle}<span class="chapter">{where.chapterTitle}</span>{/if}
    </div>
    <div class="actions">
      <button class="icon-btn" class:active={panel === "toc"} title="Contents" onclick={() => (panel = panel === "toc" ? null : "toc")}><Icon name="toc" /></button>
      <button class="icon-btn" class:active={panel === "notes"} title="Highlights & bookmarks" onclick={() => (panel = panel === "notes" ? null : "notes")}><Icon name="highlight" /></button>
      <button class="icon-btn" class:active={pageBookmarks.length > 0} title="Bookmark this page (B)" onclick={toggleBookmark}><Icon name="bookmark" filled={pageBookmarks.length > 0} /></button>
      <div class="layout-wrap">
        <button class="icon-btn" class:active={layoutMenu} title="Layout" aria-haspopup="menu" aria-expanded={layoutMenu}
          onclick={() => (layoutMenu = !layoutMenu)}>
          <Icon name={LAYOUT_ICONS[currentLayout(settings)]} />
        </button>
        {#if layoutMenu}
          <div class="layout-menu" role="menu">
            {#each LAYOUTS as l (l.id)}
              <button role="menuitemradio" aria-checked={currentLayout(settings) === l.id} class:on={currentLayout(settings) === l.id}
                onclick={() => ((layoutMenu = false), setLayout(l.id))}>
                <Icon name={LAYOUT_ICONS[l.id]} size={18} /> {l.label}
              </button>
            {/each}
          </div>
        {/if}
      </div>
      <button class="icon-btn" class:active={panel === "settings"} title="Reading settings" onclick={() => (panel = panel === "settings" ? null : "settings")}><Icon name="type" /></button>
    </div>
  </header>

  <div class="stage">
    {#if settings.flow === "paginated"}
      <button class="edge left" title={engine?.rtl ? "Next page" : "Previous page"} onclick={() => (engine?.rtl ? engine?.next() : engine?.prev())}><Icon name="left" /></button>
    {/if}
    <div class="host" bind:this={host}></div>
    {#if settings.flow === "paginated"}
      <button class="edge right" title={engine?.rtl ? "Previous page" : "Next page"} onclick={() => (engine?.rtl ? engine?.prev() : engine?.next())}><Icon name="right" /></button>
    {/if}

    {#if panel}
      <aside class="side" aria-label={panel}>
        {#if panel === "toc" && book}
          <h2>Contents</h2>
          {#if toc.length}
            <TocPanel entries={toc} current={currentHref} onselect={(h) => ((panel = null), engine?.goToHref(h))} />
          {:else}
            <p class="none">This book has no table of contents.</p>
          {/if}
        {:else if panel === "notes"}
          <h2>Annotations</h2>
          <AnnotationsPanel {annotations} onselect={jumpTo} onedit={editNote} ondelete={removeAnnotation} />
        {:else if panel === "settings"}
          <h2>Reading settings</h2>
          <SettingsPanel {fixed} />
        {/if}
      </aside>
    {/if}
  </div>

  <footer class="bar bottom">
    <input
      class="progress"
      type="range"
      min="0"
      max="1000"
      aria-label="Position in book"
      value={Math.round((scrub ?? where?.progression ?? 0) * 1000)}
      oninput={(e) => (scrub = Number((e.currentTarget as HTMLInputElement).value) / 1000)}
      onchange={(e) => {
        const f = Number((e.currentTarget as HTMLInputElement).value) / 1000;
        scrub = null;
        void engine?.goToFraction(f);
      }}
    />
    <span class="pos">
      {#if where && where.pages > 0}<span>{pageLabel}</span><span class="dot">·</span>{/if}<span>{pct}%</span>
    </span>
  </footer>

  {#if selection}
    <!-- Phones show their own Copy/Share bar for selections, so ours omits Copy there. -->
    <SelectionMenu rect={selection.rect} oncolor={(c) => createHighlight(c)} onnote={noteForSelection} oncopy={touch ? undefined : copySelection} />
  {/if}
  {#if active}
    {@const a = active.annotation}
    <SelectionMenu rect={active.rect} existing color={a.color} oncolor={(c) => recolor(a, c)} onnote={() => editNote(a)} ondelete={() => removeAnnotation(a)} />
  {/if}
  {#if editing}
    <NoteEditor quote={editing.quote} initial={editing.initial} onsave={(n) => editing?.save(n)} oncancel={() => (editing = null)} />
  {/if}
</div>

<style>
  .reader {
    height: 100%;
    display: flex;
    flex-direction: column;
    background: var(--bg);
  }
  .bar {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px 10px;
  }
  .top,
  .progress,
  .pos {
    transition: opacity 0.2s, visibility 0.2s;
  }
  /* Hidden chrome: the toolbar and slider go away; the page / % line stays, faintly. */
  .hide-chrome .top,
  .hide-chrome .progress {
    opacity: 0;
  }
  .hide-chrome .pos {
    opacity: 0.55;
  }
  /* Touch screens: hidden controls must not react to taps (a tap in the middle brings them back). */
  @media (hover: none) {
    .hide-chrome .top,
    .hide-chrome .progress {
      visibility: hidden;
      pointer-events: none;
    }
  }
  /* Mouse: hovering a hidden bar reveals it. */
  @media (hover: hover) {
    .hide-chrome .top:hover,
    .hide-chrome .top:focus-within,
    .hide-chrome .bottom:hover .progress,
    .hide-chrome .progress:focus {
      opacity: 1;
    }
  }
  .titles {
    flex: 1;
    min-width: 0;
    display: flex;
    gap: 10px;
    align-items: baseline;
    justify-content: center;
  }
  .book {
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .chapter {
    color: var(--muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .actions {
    display: flex;
    gap: 2px;
  }
  .layout-wrap {
    position: relative;
  }
  .layout-menu {
    position: absolute;
    right: 0;
    top: 40px;
    z-index: 30;
    display: flex;
    flex-direction: column;
    min-width: 170px;
    padding: 6px;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 10px;
    box-shadow: 0 8px 28px rgba(0, 0, 0, 0.18);
  }
  .layout-menu button {
    display: flex;
    align-items: center;
    gap: 10px;
    border: 0;
    background: none;
    padding: 8px 10px;
    border-radius: 6px;
    cursor: pointer;
    text-align: left;
  }
  .layout-menu button:hover {
    background: var(--panel);
  }
  .layout-menu button.on {
    color: var(--accent);
    font-weight: 600;
  }
  .stage {
    position: relative;
    flex: 1;
    display: flex;
    min-height: 0;
  }
  .host {
    position: relative;
    flex: 1;
    overflow: hidden;
  }
  .edge {
    width: 40px;
    border: 0;
    background: none;
    color: var(--muted);
    opacity: 0.35;
    cursor: pointer;
  }
  .edge:hover {
    opacity: 1;
  }
  /* Touch screens: swipes and taps turn pages, so the edge arrows go. */
  @media (pointer: coarse) {
    .edge {
      display: none;
    }
  }
  /* Phones: compact bars and bottom-sheet panels. */
  @media (max-width: 640px) {
    .chapter {
      display: none;
    }
    .titles {
      justify-content: flex-start;
    }
    .actions {
      gap: 0;
    }
    .side {
      top: auto !important;
      left: 0;
      width: 100% !important;
      max-height: 72%;
      border-left: 0 !important;
      border-top: 1px solid var(--border);
      border-radius: 16px 16px 0 0;
      box-shadow: 0 -12px 32px rgba(0, 0, 0, 0.18) !important;
    }
    .pos {
      min-width: 0;
    }
  }
  .side {
    position: absolute;
    top: 0;
    right: 0;
    bottom: 0;
    width: min(360px, 92vw);
    overflow: auto;
    background: var(--bg);
    border-left: 1px solid var(--border);
    box-shadow: -12px 0 32px rgba(0, 0, 0, 0.12);
    z-index: 10;
  }
  .none {
    color: var(--muted);
    padding: 0 18px;
  }
  .side h2 {
    font-size: 15px;
    margin: 14px 18px 6px;
  }
  .bottom {
    gap: 14px;
    padding: 6px 18px 10px;
  }
  .progress {
    flex: 1;
    accent-color: var(--accent);
  }
  .dot {
    margin: 0 6px;
  }
  .pos {
    font-size: 12.5px;
    color: var(--muted);
    min-width: 130px;
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
</style>

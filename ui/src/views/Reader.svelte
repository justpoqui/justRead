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
  import SettingsScreen from "../components/SettingsScreen.svelte";
  import PositionSheet from "../components/PositionSheet.svelte";
  import { fullPosition, nextMode, progressLabel, type ProgressInput } from "../lib/reader/progress";
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
  import { onBack } from "../lib/back";
  import { ReadingTracker } from "../lib/stats/tracker";
  import ScreenGuard from "../components/ScreenGuard.svelte";

  let { id, onclose }: { id: string; onclose: () => void } = $props();

  let host = $state<HTMLDivElement | undefined>();
  let book = $state<OpenedBook | null>(null);
  // $state.raw: the engine object itself is not made deeply reactive.
  let engine = $state.raw<Engine | null>(null);
  let toc = $state<TocEntry[]>([]);
  const fixed = $derived(book?.manifest.layout === "fixed");
  let where = $state<RelocateEvent | null>(null);
  let annotations = $state<Annotation[]>([]);
  let panel = $state<"toc" | "notes" | "settings" | "position" | null>(null);
  /** The full Settings screen (⚙). */
  let settingsOpen = $state(false);
  /** End of the book reached: offer to move it to the Finished shelf (once per visit). */
  let endPrompt = $state<"show" | "done" | null>(null);
  /** The reader's screen timeout (dim / soft lock). */
  let screen = $state<ScreenGuard | undefined>();
  /** Where the reader was before a jump from the Position popup ("Back to where you were"). */
  let returnTo = $state<string | null>(null);
  let chrome = $state(true);
  let selection = $state<SelectionEvent | null>(null);
  let active = $state<{ annotation: Annotation; rect: DOMRect } | null>(null);
  let editing = $state<{ quote: string; initial: string; save: (note: string) => Promise<void> } | null>(null);
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

  // Esc and the Android back button close the topmost thing that is open;
  // with nothing open they leave the book. Always consumes the press.
  function back(): boolean {
    // The screen guard first: while dim or locked, back must not leave the book.
    if (screen?.back()) return true;
    if (editing) editing = null;
    else if (settingsOpen) settingsOpen = false;
    else if (layoutMenu) layoutMenu = false;
    else if (selection || active) {
      selection = null;
      active = null;
      engine?.clearSelection();
    } else if (panel) panel = null;
    else onclose();
    return true;
  }
  onMount(() => onBack(back));

  // Reading time (Phase 8): only active reading counts, reported about once a minute.
  const tracker = new ReadingTracker((r) => void api.recordReading(id, r.day, r.seconds, r.progress).catch(() => {}));
  onMount(() => {
    const timer = setInterval(() => tracker.tick(), 5000);
    const vis = () => tracker.setVisible(document.visibilityState === "visible");
    document.addEventListener("visibilitychange", vis);
    return () => {
      clearInterval(timer);
      document.removeEventListener("visibilitychange", vis);
      tracker.flush();
    };
  });

  async function markFinished() {
    endPrompt = "done";
    try {
      await api.setBookStatus(id, "finished");
      if (book) book.book.status = "finished";
      toast("Moved to Finished");
    } catch (e) {
      toast(String(e), "error");
    }
  }

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
          tracker.moved(e.progression);
          screen?.activity();
          // The last page of the book: offer the Finished shelf.
          const m = book?.manifest;
          const atEnd = e.progression >= 0.995 || (!!m && m.layout !== "fixed" && e.spine === m.spine.length - 1 && e.pages > 0 && e.page === e.pages);
          if (atEnd && endPrompt === null && book?.book.status !== "finished") endPrompt = "show";
          clearTimeout(saveTimer);
          // Debounced: saving on every page turn would be wasteful.
          saveTimer = setTimeout(() => void api.saveProgress(id, formatLocator(e.anchor), e.progression), 600);
        });
        engine.on("selection", (s) => {
          tracker.active();
          screen?.activity();
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
        // Keys pressed inside the book page: the screen guard sees them first.
        engine.on("key", (e) => {
          if (!screen?.key(e)) onKey(e);
        });
        engine.on("center", () => {
          screen?.activity();
          chrome = !chrome;
        });
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
        back();
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

  // ----- page indicator ---------------------------------------------------
  const progressInput = $derived.by((): ProgressInput | null => {
    if (!where || !book) return null;
    const sp = book.manifest.spine;
    return {
      fixed,
      paginated: settings.flow === "paginated",
      page: where.page,
      pages: where.pages,
      visiblePages: where.visiblePages,
      chapterFraction: where.chapterFraction,
      bookFraction: where.progression,
      chapterChars: sp[where.spine]?.chars ?? 0,
      charsAfterChapter: sp.slice(where.spine + 1).reduce((n, x) => n + x.chars, 0),
    };
  });
  const posLabel = $derived(progressInput ? progressLabel(settings.progressDisplay, progressInput) : "");
  const posFull = $derived(progressInput ? fullPosition(progressInput) : "");

  /** Tap the indicator: next view, remembered for next time. */
  function cycleProgress() {
    if (!progressInput) return;
    settings.progressDisplay = nextMode(settings.progressDisplay, progressInput);
    saveSettings();
  }

  // ----- position popup -----------------------------------------------------
  /** Jumps from the popup remember where the reader was, once per visit. */
  function jump(go: () => Promise<void> | undefined) {
    if (where && returnTo === null) returnTo = formatLocator(where.anchor);
    void go();
  }
  function goBack() {
    const loc = returnTo;
    returnTo = null;
    if (loc) void engine?.goToLocator(loc);
  }
  function togglePanel(p: "toc" | "notes" | "settings" | "position") {
    layoutMenu = false;
    if (p === "position" && panel !== "position") returnTo = null;
    panel = panel === p ? null : p;
  }

  const toolbarTop = $derived(settings.toolbarPosition === "top");
</script>

<svelte:window onkeydown={onKey} />

<div class="reader" class:hide-chrome={!chrome && !panel && !settingsOpen}>
  {#snippet toolbar(up: boolean)}
    <div class="actions">
      {#each settings.toolbar as item (item)}
        {#if item === "toc"}
          <button class="icon-btn" class:active={panel === "toc"} title="Contents" onclick={() => togglePanel("toc")}><Icon name="toc" /></button>
        {:else if item === "notes"}
          <button class="icon-btn" class:active={panel === "notes"} title="Highlights & bookmarks" onclick={() => togglePanel("notes")}><Icon name="highlight" /></button>
        {:else if item === "bookmark"}
          <button class="icon-btn" class:active={pageBookmarks.length > 0} title="Bookmark this page (B)" onclick={toggleBookmark}><Icon name="bookmark" filled={pageBookmarks.length > 0} /></button>
        {:else if item === "layout"}
          <div class="layout-wrap">
            <button class="icon-btn" class:active={layoutMenu} title="Layout" aria-haspopup="menu" aria-expanded={layoutMenu}
              onclick={() => (layoutMenu = !layoutMenu)}>
              <Icon name={LAYOUT_ICONS[currentLayout(settings)]} />
            </button>
            {#if layoutMenu}
              <div class="layout-menu" class:up role="menu">
                {#each LAYOUTS as l (l.id)}
                  <button role="menuitemradio" aria-checked={currentLayout(settings) === l.id} class:on={currentLayout(settings) === l.id}
                    onclick={() => ((layoutMenu = false), setLayout(l.id))}>
                    <Icon name={LAYOUT_ICONS[l.id]} size={18} /> {l.label}
                  </button>
                {/each}
              </div>
            {/if}
          </div>
        {:else if item === "text"}
          <button class="icon-btn" class:active={panel === "settings"} title="Reading settings" onclick={() => togglePanel("settings")}><Icon name="type" /></button>
        {:else if item === "position"}
          <button class="icon-btn" class:active={panel === "position"} title="Go to position" onclick={() => togglePanel("position")}><Icon name="position" /></button>
        {/if}
      {/each}
    </div>
  {/snippet}

  <header class="bar top">
    <button class="icon-btn" title="Back to library (Esc)" onclick={onclose}><Icon name="back" /></button>
    <div class="titles">
      <span class="book">{title}</span>
      {#if where?.chapterTitle}<span class="chapter">{where.chapterTitle}</span>{/if}
    </div>
    {#if toolbarTop}{@render toolbar(false)}{/if}
    <button class="icon-btn" title="Settings" onclick={() => ((layoutMenu = false), (settingsOpen = true))}><Icon name="cog" /></button>
  </header>

  <div class="stage">
    {#if settings.flow === "paginated"}
      <button class="edge left" title={engine?.rtl ? "Next page" : "Previous page"} onclick={() => (engine?.rtl ? engine?.next() : engine?.prev())}><Icon name="left" /></button>
    {/if}
    <div class="host" bind:this={host}></div>
    {#if settings.flow === "paginated"}
      <button class="edge right" title={engine?.rtl ? "Previous page" : "Next page"} onclick={() => (engine?.rtl ? engine?.prev() : engine?.next())}><Icon name="right" /></button>
    {/if}

    {#if panel === "position" && where && book}
      <PositionSheet
        manifest={book.manifest}
        {where}
        {fixed}
        canGoBack={returnTo !== null}
        onchapter={(i) => jump(() => engine?.goToHref(book!.manifest.spine[i]!.href))}
        onchapterfraction={(f) => jump(() => engine?.goToChapterFraction(f))}
        onbookfraction={(f) => jump(() => engine?.goToFraction(f))}
        onback={goBack}
        onclose={() => (panel = null)}
      />
    {:else if panel && panel !== "position"}
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
          <AnnotationsPanel bookId={id} {annotations} onselect={jumpTo} onedit={editNote} ondelete={removeAnnotation} />
        {:else if panel === "settings"}
          <h2>Reading settings</h2>
          <SettingsPanel {fixed} />
        {/if}
      </aside>
    {/if}
  </div>

  <footer class="bar bottom">
    {#if !toolbarTop}{@render toolbar(true)}{/if}
    <button class="pos" title="Tap to switch: pages left / percent" onclick={cycleProgress}>
      <span class="pos-label" aria-hidden="true">{posLabel}</span>
      <span class="sr-only pos-full">{posFull}</span>
    </button>
    <button class="icon-btn hide-btn" title="Hide controls" onclick={() => ((layoutMenu = false), (panel = null), (chrome = false))}><Icon name="down" /></button>
  </footer>

  {#if endPrompt === "show"}
    <div class="end-prompt" role="status">
      <span>You’ve reached the end of the book.</span>
      <button class="primary" onclick={markFinished}>Mark as finished</button>
      <button onclick={() => (endPrompt = "done")}>Not now</button>
    </div>
  {/if}

  {#if !chrome && !panel && !settingsOpen}
    <button class="show-handle" title="Show controls" onclick={() => (chrome = true)}><Icon name="up" size={18} /></button>
  {/if}
  {#if settingsOpen}
    <SettingsScreen {fixed} onclose={() => (settingsOpen = false)} />
  {/if}
  <ScreenGuard bind:this={screen} enabled={settings.screenGuard} dimAfter={settings.dimAfter} lockAfter={settings.lockAfter} />

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
    /* Overlays (Settings screen, handle) are placed inside the reader, which sits
       inside the safe-area padding, so phone status bars never cover them. */
    position: relative;
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
  .bottom > :global(*) {
    transition: opacity 0.2s, visibility 0.2s;
  }
  /* Hidden controls: both bars go away and nothing hidden reacts to taps
     or clicks. The page indicator stays, faintly; the handle brings them back. */
  .hide-chrome .top,
  .hide-chrome .bottom > :global(:not(.pos)) {
    opacity: 0;
    visibility: hidden;
    pointer-events: none;
  }
  .hide-chrome .pos {
    opacity: 0.55;
    pointer-events: none;
  }
  .end-prompt {
    position: absolute;
    left: 50%;
    bottom: 64px;
    transform: translateX(-50%);
    z-index: 22;
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    justify-content: center;
    gap: 8px;
    width: max-content;
    max-width: calc(100% - 24px);
    padding: 10px 14px;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 12px;
    box-shadow: 0 8px 28px rgba(0, 0, 0, 0.18);
  }
  .end-prompt button {
    border: 1px solid var(--border);
    border-radius: 8px;
    background: none;
    padding: 5px 12px;
    cursor: pointer;
  }
  .end-prompt .primary {
    background: var(--accent);
    border-color: var(--accent);
    color: #fff;
  }
  .show-handle {
    position: absolute;
    right: 10px;
    bottom: 8px;
    z-index: 20;
    width: 34px;
    height: 34px;
    display: grid;
    place-items: center;
    border-radius: 50%;
    border: 1px solid var(--border);
    background: var(--bg);
    color: var(--muted);
    opacity: 0.7;
    cursor: pointer;
  }
  .show-handle:hover {
    opacity: 1;
  }
  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip: rect(0 0 0 0);
    white-space: nowrap;
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
  .layout-menu.up {
    top: auto;
    bottom: 40px;
    left: 0;
    right: auto;
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
    /* Room is tight next to the buttons: let the indicator use two short lines. */
    .bottom .pos {
      white-space: normal;
      line-height: 1.25;
      font-size: 12px;
    }
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
    position: relative;
    gap: 8px;
    padding: 4px 10px 8px;
  }
  .pos {
    flex: 1;
    min-width: 0;
    border: 0;
    background: none;
    padding: 6px 4px;
    cursor: pointer;
    font: inherit;
    font-size: 12.5px;
    color: var(--muted);
    text-align: right;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .hide-btn {
    flex: none;
  }
</style>

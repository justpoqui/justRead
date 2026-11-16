<!-- Copyright 2026 The justRead Authors. SPDX-License-Identifier: Apache-2.0 -->
<script lang="ts">
  import { onMount } from "svelte";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { listen } from "@tauri-apps/api/event";
  import Icon from "../components/Icon.svelte";
  import BookCover from "../components/BookCover.svelte";
  import BookSheet from "../components/BookSheet.svelte";
  import UpdateBanner from "../components/UpdateBanner.svelte";
  import Avatar from "../components/Avatar.svelte";
  import { profile } from "../lib/profile.svelte";
  import { api } from "../lib/ipc/api";
  import type { BookSummary, Collection, ImportResult, SortKey } from "../lib/ipc/types";
  import { libraryPrefs, saveLibraryPrefs } from "../lib/settings/store.svelte";
  import { onBack } from "../lib/back";
  import { toast } from "../lib/stores/toasts.svelte";
  import { packageUrl } from "../lib/ipc/api";

  let { onopen, onstats, onprofile }: { onopen: (id: string) => void; onstats: () => void; onprofile: () => void } = $props();

  let books: BookSummary[] = $state([]);
  let search = $state("");
  let importing = $state(false);
  let dragging = $state(false);
  let loaded = $state(false);
  // Phase 8: collections, the "Continue reading" row and the book options sheet.
  let collections: Collection[] = $state([]);
  let continueReading: BookSummary[] = $state([]);
  let sheetFor = $state<string | null>(null);
  const sheetBook = $derived(books.find((b) => b.id === sheetFor) ?? continueReading.find((b) => b.id === sheetFor) ?? null);
  const SHELF_CHIPS = [
    { id: "all", label: "All" },
    { id: "reading", label: "Reading" },
    { id: "unread", label: "Unread" },
    { id: "finished", label: "Finished" },
  ] as const;
  const filtered = $derived(libraryPrefs.shelf !== "all" || libraryPrefs.collection !== null);
  // Android back: clear an active search first; otherwise let the app close.
  onMount(() => onBack(() => (search ? ((search = ""), true) : false)));

  const SORTS: { key: SortKey; label: string }[] = [
    { key: "recent", label: "Recently read" },
    { key: "title", label: "Title" },
    { key: "author", label: "Author" },
    { key: "series", label: "Series" },
    { key: "progress", label: "Progress" },
    { key: "added", label: "Date added" },
  ];

  // PDFs have no cover image inside; render page 1 once, in the background.
  const coverAttempted = new Set<string>();
  async function ensurePdfCovers(list: BookSummary[]) {
    for (const b of list) {
      if (b.format !== "pdf" || b.cover || coverAttempted.has(b.id)) continue;
      coverAttempted.add(b.id);
      try {
        const { renderPdfCover } = await import("../lib/fixed/sources");
        const bytes = await renderPdfCover(packageUrl(b.id, "content/book.pdf"));
        const cover = await api.setCover(b.id, bytes);
        books = books.map((x) => (x.id === b.id ? { ...x, cover } : x));
      } catch (e) {
        console.warn("cover generation failed", b.id, e);
      }
    }
  }

  // STEP 1: (re)load whenever search or sorting changes.
  async function refresh() {
    try {
      // A deleted collection can't stay selected.
      collections = await api.listCollections();
      if (libraryPrefs.collection && !collections.some((c) => c.id === libraryPrefs.collection)) libraryPrefs.collection = null;
      books = await api.listBooks({
        search: search || null,
        sort: libraryPrefs.sort,
        descending: libraryPrefs.descending,
        status: libraryPrefs.shelf === "all" ? null : libraryPrefs.shelf,
        collection: libraryPrefs.collection,
      });
      continueReading = (await api.listBooks({ status: "reading", sort: "recent", descending: true }))
        .filter((b) => b.lastOpenedAt !== null)
        .slice(0, 12);
      void ensurePdfCovers(books);
    } catch (e) {
      toast(String(e), "error");
    } finally {
      loaded = true;
    }
  }
  $effect(() => {
    void search;
    void libraryPrefs.sort;
    void libraryPrefs.descending;
    void libraryPrefs.shelf;
    void libraryPrefs.collection;
    void refresh();
  });

  // STEP 2: importing (file picker or drag-and-drop share this path).
  async function importPaths(paths: string[], openAfter = false) {
    if (paths.length === 0) return;
    importing = true;
    try {
      const results = await api.importBooks(paths);
      report(results);
      await refresh();
      // "Open with justRead": go straight to the (first) book.
      const first = results.find((r) => r.outcome)?.outcome?.book;
      if (openAfter && first) onopen(first.id);
    } catch (e) {
      toast(String(e), "error");
    } finally {
      importing = false;
    }
  }

  /** Files the OS asked us to open (launch arguments, share sheet, "Open with"). */
  async function openPending() {
    const files = (await api.takePendingOpen().catch(() => null)) ?? [];
    if (files.length) await importPaths(files, true);
  }

  function report(results: ImportResult[]) {
    const added = results.filter((r) => r.outcome?.status === "imported").length;
    const dupes = results.filter((r) => r.outcome?.status === "duplicate").length;
    for (const r of results.filter((r) => r.error)) {
      const name = r.path.split(/[\\/]/).pop();
      // DRM gets a clear, specific message; we never try to bypass it.
      toast(r.drm ? `“${name}” is DRM-protected and can’t be opened.` : `Couldn’t import “${name}”: ${r.error}`, "error", 8000);
    }
    if (added) toast(`${added} book${added > 1 ? "s" : ""} added`);
    if (dupes) toast(`${dupes} already in your library`);
  }

  async function pickFiles() {
    // The picker is opened by Rust, which imports exactly what was chosen.
    importing = true;
    try {
      report(await api.pickAndImport());
      await refresh();
    } catch (e) {
      toast(String(e), "error");
    } finally {
      importing = false;
    }
  }

  async function remove(book: BookSummary) {
    if (!confirm(`Remove “${book.title}” and its highlights from this device?`)) return;
    try {
      await api.deleteBook(book.id);
      sheetFor = null;
      await refresh();
    } catch (e) {
      toast(String(e), "error");
    }
  }

  // STEP 3: drag-and-drop from the desktop (Tauri delivers real file paths).
  // Docs: https://v2.tauri.app/reference/javascript/api/namespacewebview/#ondragdropevent
  onMount(() => {
    void getCurrentWindow().setTitle("justRead");
    let unlisten: (() => void) | undefined;
    getCurrentWebview()
      .onDragDropEvent((e) => {
        if (e.payload.type === "enter" || e.payload.type === "over") dragging = true;
        else if (e.payload.type === "leave") dragging = false;
        else if (e.payload.type === "drop") {
          dragging = false;
          void importPaths(e.payload.paths);
        }
      })
      .then((u) => (unlisten = u))
      .catch(() => {});
    // Refresh when Rust changes the library (e.g. a file opened with "Open with").
    let unlistenLib: (() => void) | undefined;
    listen("library-changed", () => void refresh())
      .then((u) => (unlistenLib = u))
      .catch(() => {});
    // "Open with": files queued before we started, and any that arrive later.
    void openPending();
    let unlistenOpen: (() => void) | undefined;
    listen("open-files", () => void openPending())
      .then((u) => (unlistenOpen = u))
      .catch(() => {});
    return () => {
      unlisten?.();
      unlistenLib?.();
      unlistenOpen?.();
    };
  });

  function setSort(key: SortKey) {
    libraryPrefs.sort = key;
    saveLibraryPrefs();
  }

  function setShelf(id: LibraryShelf) {
    libraryPrefs.shelf = id;
    libraryPrefs.collection = null;
    saveLibraryPrefs();
  }
  function setCollection(id: string) {
    libraryPrefs.collection = libraryPrefs.collection === id ? null : id;
    libraryPrefs.shelf = "all";
    saveLibraryPrefs();
  }
  async function newCollection() {
    const name = prompt("Name of the new collection");
    if (!name?.trim()) return;
    try {
      const c = await api.createCollection(name);
      libraryPrefs.collection = c.id;
      libraryPrefs.shelf = "all";
      saveLibraryPrefs();
    } catch (e) {
      toast(String(e), "error");
    }
  }
  async function renameCollection(c: Collection) {
    const name = prompt("Rename collection", c.name);
    if (!name?.trim() || name.trim() === c.name) return;
    try {
      await api.renameCollection(c.id, name);
      await refresh();
    } catch (e) {
      toast(String(e), "error");
    }
  }
  async function deleteCollection(c: Collection) {
    if (!confirm(`Delete the collection “${c.name}”? Its books stay in your library.`)) return;
    await api.deleteCollection(c.id).catch((e) => toast(String(e), "error"));
    libraryPrefs.collection = null;
    saveLibraryPrefs();
  }
  type LibraryShelf = (typeof SHELF_CHIPS)[number]["id"];
  const activeCollection = $derived(collections.find((c) => c.id === libraryPrefs.collection) ?? null);

  function pct(b: BookSummary) {
    return Math.round(b.progression * 100);
  }
</script>

<div class="library">
  <header>
    <div class="brand"><Icon name="book" size={22} /> <span>justRead</span></div>
    <label class="search">
      <Icon name="search" size={16} />
      <span class="sr-only">Search</span>
      <input type="search" placeholder="Search title, author, series" bind:value={search} />
    </label>
    <div class="tools">
      <label class="sr-only" for="sort">Sort by</label>
      <select id="sort" value={libraryPrefs.sort} onchange={(e) => setSort((e.currentTarget as HTMLSelectElement).value as SortKey)}>
        {#each SORTS as s (s.key)}<option value={s.key}>{s.label}</option>{/each}
      </select>
      <button
        class="icon-btn"
        title={libraryPrefs.descending ? "Descending" : "Ascending"}
        onclick={() => ((libraryPrefs.descending = !libraryPrefs.descending), saveLibraryPrefs())}
      >
        <Icon name="sort" />
      </button>
      <button
        class="icon-btn"
        title={libraryPrefs.view === "grid" ? "List view" : "Grid view"}
        onclick={() => ((libraryPrefs.view = libraryPrefs.view === "grid" ? "list" : "grid"), saveLibraryPrefs())}
      >
        <Icon name={libraryPrefs.view === "grid" ? "list" : "grid"} />
      </button>
      <button class="icon-btn" title="Reading stats" onclick={onstats}><Icon name="chart" /></button>
      <button class="profile-btn" title="Profile" onclick={onprofile}>
        <Avatar name={profile.name} emoji={profile.emoji} color={profile.color} size={32} />
      </button>
      <button class="btn primary" onclick={pickFiles} disabled={importing}>
        <Icon name="plus" size={16} />
        {importing ? "Importing…" : "Import"}
      </button>
    </div>
  </header>

  <main>
    <UpdateBanner />
    {#if !search && !filtered && continueReading.length}
      <section class="continue" aria-label="Continue reading">
        <h2>Continue reading</h2>
        <ul>
          {#each continueReading as b (b.id)}
            <li>
              <button class="cont" onclick={() => onopen(b.id)} title="Continue “{b.title}”">
                <BookCover book={b} />
                <span class="title">{b.title}</span>
                <span class="bar" aria-label="{pct(b)}% read"><span style="width:{pct(b)}%"></span></span>
              </button>
            </li>
          {/each}
        </ul>
      </section>
    {/if}

    {#if loaded}
      <nav class="chips" aria-label="Shelves and collections">
        {#each SHELF_CHIPS as s (s.id)}
          <button class:on={libraryPrefs.shelf === s.id && !libraryPrefs.collection} aria-pressed={libraryPrefs.shelf === s.id && !libraryPrefs.collection} onclick={() => setShelf(s.id)}>{s.label}</button>
        {/each}
        {#if collections.length}<span class="sep" aria-hidden="true"></span>{/if}
        {#each collections as c (c.id)}
          <button class="coll" class:on={libraryPrefs.collection === c.id} aria-pressed={libraryPrefs.collection === c.id} onclick={() => setCollection(c.id)}>{c.name} <span class="n">{c.count}</span></button>
        {/each}
        <button class="add" title="New collection" onclick={newCollection}><Icon name="plus" size={14} /> Collection</button>
        {#if activeCollection}
          <span class="coll-tools">
            <button class="icon-btn" title="Rename collection" onclick={() => renameCollection(activeCollection)}><Icon name="note" size={15} /></button>
            <button class="icon-btn" title="Delete collection" onclick={() => deleteCollection(activeCollection)}><Icon name="trash" size={15} /></button>
          </span>
        {/if}
      </nav>
    {/if}

    {#if loaded && books.length === 0 && (search || filtered)}
      <div class="empty">
        {#if search}
          <p>No books match “{search}”.</p>
        {:else if activeCollection}
          <p>“{activeCollection.name}” is empty. Add books with the ⋯ button on a book.</p>
        {:else}
          <p>No books on this shelf yet.</p>
        {/if}
      </div>
    {:else if loaded && books.length === 0}
      <div class="empty">
        {#if search}
          <p>No books match “{search}”.</p>
        {:else}
          <Icon name="book" size={48} />
          <h2>Your library is empty</h2>
          <p>Import EPUB, MOBI/AZW3, FB2, DOCX, HTML, TXT, PDF or comic (CBZ/CB7) files, or drop them anywhere on this window.</p>
          <button class="btn primary" onclick={pickFiles}><Icon name="plus" size={16} /> Import books</button>
        {/if}
      </div>
    {:else if libraryPrefs.view === "grid"}
      <ul class="grid">
        {#each books as b (b.id)}
          <li>
            <button class="card" onclick={() => onopen(b.id)} title={b.title}>
              <BookCover book={b} />
              <span class="title">{b.title}</span>
              <span class="author">{b.authors.join(", ") || "Unknown author"}</span>
              {#if b.progression > 0}
                <span class="bar" aria-label="{pct(b)}% read"><span style="width:{pct(b)}%"></span></span>
              {/if}
            </button>
            <button class="del icon-btn" title="Book options" onclick={() => (sheetFor = b.id)}><Icon name="more" size={18} /></button>
          </li>
        {/each}
      </ul>
    {:else}
      <table class="list">
        <thead><tr><th></th><th>Title</th><th>Author</th><th>Series</th><th>Progress</th><th></th></tr></thead>
        <tbody>
          {#each books as b (b.id)}
            <tr ondblclick={() => onopen(b.id)}>
              <td class="thumb"><BookCover book={b} small /></td>
              <td><button class="link" onclick={() => onopen(b.id)}>{b.title}</button></td>
              <td>{b.authors.join(", ")}</td>
              <td>{b.series ? `${b.series}${b.seriesIndex != null ? ` #${b.seriesIndex}` : ""}` : ""}</td>
              <td>{pct(b)}%</td>
              <td><button class="icon-btn" title="Book options" onclick={() => (sheetFor = b.id)}><Icon name="more" size={18} /></button></td>
            </tr>
          {/each}
        </tbody>
      </table>
    {/if}
  </main>

  {#if sheetBook}
    {@const b = sheetBook}
    <BookSheet book={b} {collections} onchange={() => void refresh()} onremove={() => remove(b)} onclose={() => (sheetFor = null)} />
  {/if}

  {#if dragging}
    <div class="drop">Drop books to import</div>
  {/if}
</div>

<style>
  .library {
    height: 100%;
    display: flex;
    flex-direction: column;
  }
  header {
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 12px 20px;
    border-bottom: 1px solid var(--border);
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 8px;
    font-weight: 650;
    font-size: 16px;
  }
  .search {
    flex: 1;
    max-width: 420px;
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--muted);
  }
  .search input {
    flex: 1;
  }
  .tools {
    margin-left: auto;
    display: flex;
    align-items: center;
    gap: 4px;
  }
  main {
    flex: 1;
    overflow: auto;
    padding: 24px 20px 48px;
  }
  .grid {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
    gap: 28px 22px;
  }
  .grid li {
    position: relative;
  }
  .card {
    width: 100%;
    display: flex;
    flex-direction: column;
    gap: 4px;
    border: 0;
    background: none;
    padding: 0;
    text-align: left;
    cursor: pointer;
  }
  .card:hover :global(.cover) {
    transform: translateY(-2px);
    box-shadow: 0 10px 24px rgba(0, 0, 0, 0.22);
  }
  .title {
    margin-top: 6px;
    font-weight: 600;
    line-height: 1.25;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  .author {
    color: var(--muted);
    font-size: 13px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .bar {
    height: 3px;
    background: var(--border);
    border-radius: 2px;
    overflow: hidden;
    margin-top: 4px;
  }
  .bar span {
    display: block;
    height: 100%;
    background: var(--accent);
  }
  @media (hover: none) {
    .del {
      opacity: 0.85 !important;
    }
  }
  .del {
    position: absolute;
    top: 6px;
    right: 6px;
    background: rgba(0, 0, 0, 0.55);
    color: #fff;
    opacity: 0;
    transition: opacity 0.15s;
  }
  .grid li:hover .del,
  .del:focus-visible {
    opacity: 1;
  }
  /* Phase 8: Continue reading row and shelf/collection chips. */
  .continue h2 {
    font-size: 14px;
    margin: 0 0 10px;
  }
  .continue ul {
    list-style: none;
    margin: 0 0 18px;
    padding: 0 0 6px;
    display: flex;
    gap: 16px;
    overflow-x: auto;
    scroll-snap-type: x proximity;
  }
  .continue li {
    flex: none;
    width: 110px;
    scroll-snap-align: start;
  }
  .cont {
    width: 100%;
    display: flex;
    flex-direction: column;
    gap: 4px;
    border: 0;
    background: none;
    padding: 0;
    text-align: left;
    cursor: pointer;
  }
  .cont .title {
    font-size: 12.5px;
  }
  .chips {
    display: flex;
    gap: 6px;
    align-items: center;
    overflow-x: auto;
    margin: 0 0 18px;
    padding-bottom: 4px;
  }
  .chips > button {
    flex: none;
    border: 1px solid var(--border);
    background: none;
    border-radius: 999px;
    padding: 5px 12px;
    cursor: pointer;
    white-space: nowrap;
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }
  .chips > button.on {
    background: var(--accent);
    border-color: var(--accent);
    color: #fff;
  }
  .chips .n {
    opacity: 0.7;
    font-size: 12px;
  }
  .chips .add {
    color: var(--muted);
    border-style: dashed;
  }
  .chips .sep {
    flex: none;
    width: 1px;
    height: 20px;
    background: var(--border);
    margin: 0 4px;
  }
  .coll-tools {
    display: flex;
    flex: none;
  }
  .profile-btn {
    border: 0;
    background: none;
    padding: 2px;
    border-radius: 50%;
    cursor: pointer;
    display: grid;
  }
  .profile-btn:focus-visible {
    outline: 2px solid var(--accent);
  }
  .list {
    width: 100%;
    border-collapse: collapse;
  }
  .list th {
    text-align: left;
    font-weight: 500;
    color: var(--muted);
    padding: 6px 8px;
    border-bottom: 1px solid var(--border);
  }
  .list td {
    padding: 6px 8px;
    border-bottom: 1px solid var(--border);
  }
  .thumb {
    width: 44px;
  }
  .link {
    border: 0;
    background: none;
    padding: 0;
    font-weight: 600;
    cursor: pointer;
    text-align: left;
  }
  .empty {
    max-width: 420px;
    margin: 12vh auto 0;
    text-align: center;
    color: var(--muted);
  }
  .empty h2 {
    color: var(--fg);
    margin: 12px 0 4px;
  }
  /* Phones: two-row header, smaller covers. */
  @media (max-width: 640px) {
    header {
      flex-wrap: wrap;
      gap: 8px 10px;
      padding: 10px 12px;
    }
    .search {
      order: 3;
      flex-basis: 100%;
      max-width: none;
    }
    main {
      padding: 16px 12px 40px;
    }
    .grid {
      grid-template-columns: repeat(auto-fill, minmax(104px, 1fr));
      gap: 20px 12px;
    }
  }
  .drop {
    position: fixed;
    inset: 12px;
    border: 3px dashed var(--accent);
    border-radius: 16px;
    display: grid;
    place-items: center;
    font-size: 22px;
    font-weight: 600;
    background: color-mix(in srgb, var(--bg) 85%, transparent);
    pointer-events: none;
  }
</style>

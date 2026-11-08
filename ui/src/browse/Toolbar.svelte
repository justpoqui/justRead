<!-- Copyright 2026 The justRead Authors. SPDX-License-Identifier: Apache-2.0 -->
<!--
  The justBrowse toolbar. It talks to Rust through the browse_* commands
  only; the web page below it lives in a separate webview with no IPC.
  Wording rule: justBrowse is "privacy-hardened" — never "safe" or "secure"
  in absolute terms.
-->
<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";

  interface PageState { url: string; loading: boolean; secure: boolean; blocked: number }
  interface Download { status: string; name: string; message: string }

  let address = $state("");
  let editing = $state(false);
  let page = $state<PageState>({ url: "", loading: false, secure: true, blocked: 0 });
  let blocked = $state(0);
  let httpWarning = $state<string | null>(null);
  let download = $state<Download | null>(null);
  let showSettings = $state(false);
  let search = $state("");
  let searchDraft = $state("");
  let error = $state("");
  let input: HTMLInputElement | undefined = $state();

  const BOOKMARKS = [
    { label: "Project Gutenberg", url: "https://www.gutenberg.org" },
    { label: "Standard Ebooks", url: "https://standardebooks.org" },
    { label: "Wikisource", url: "https://wikisource.org" },
    { label: "Feedbooks (public domain)", url: "https://www.feedbooks.com/publicdomain" },
  ];
  const PRESETS = [
    { label: "DuckDuckGo", template: "https://duckduckgo.com/?q={q}" },
    { label: "DuckDuckGo (no JavaScript)", template: "https://html.duckduckgo.com/html/?q={q}" },
  ];

  // STEP 1: initial config and live events from Rust.
  onMount(() => {
    const unlisten: (() => void)[] = [];
    (async () => {
      const cfg = await invoke<{ search: string; blocked: number }>("browse_config");
      search = searchDraft = cfg.search;
      blocked = cfg.blocked;
      unlisten.push(await listen<PageState>("browse-state", (e) => {
        page = e.payload;
        blocked = e.payload.blocked;
        if (!editing && e.payload.url !== "about:blank") address = e.payload.url;
        if (!e.payload.loading) httpWarning = null;
      }));
      unlisten.push(await listen<number>("browse-blocked", (e) => (blocked = e.payload)));
      unlisten.push(await listen<string>("browse-http-warning", (e) => (httpWarning = e.payload)));
      unlisten.push(await listen<Download>("browse-download", (e) => (download = e.payload)));
      input?.focus();
    })();
    return () => unlisten.forEach((u) => u());
  });

  async function go(target = address) {
    error = "";
    editing = false;
    try {
      address = await invoke<string>("browse_navigate", { input: target });
    } catch (e) {
      error = String(e);
    }
  }

  async function allowHttp() {
    if (!httpWarning) return;
    const url = httpWarning;
    httpWarning = null;
    await invoke("browse_allow_http", { url });
  }

  async function saveSearch(template: string) {
    try {
      await invoke("browse_set_search", { template });
      search = searchDraft = template;
      showSettings = false;
      error = "";
    } catch (e) {
      error = String(e);
    }
  }

  const host = $derived.by(() => {
    try {
      return new URL(page.url).host;
    } catch {
      return "";
    }
  });
</script>

<div class="bar">
  <div class="row">
    <button class="btn" title="Back" onclick={() => invoke("browse_history", { step: -1 })}>‹</button>
    <button class="btn" title="Forward" onclick={() => invoke("browse_history", { step: 1 })}>›</button>
    <button class="btn" title="Reload" onclick={() => invoke("browse_reload")}>{page.loading ? "×" : "↻"}</button>
    <span class="lock" class:insecure={!page.secure && page.url.startsWith("http:")} title={page.secure ? "Encrypted connection (HTTPS)" : "Not encrypted"}>
      {page.url.startsWith("http:") ? "Not encrypted" : "🔒"}
    </span>
    <input
      bind:this={input}
      class="address"
      aria-label="Address or search"
      placeholder="Search or enter an address"
      bind:value={address}
      onfocus={() => (editing = true)}
      onblur={() => (editing = false)}
      onkeydown={(e) => e.key === "Enter" && go()}
    />
    <span class="shield" title="Trackers and ads blocked in this session">🛡 {blocked}</span>
    <button class="btn" class:on={showSettings} title="Search engine" onclick={() => (showSettings = !showSettings)}>⚙</button>
  </div>

  <div class="row second">
    {#if httpWarning}
      <div class="warning" role="alert">
        <strong>{new URL(httpWarning).host}</strong> doesn’t use HTTPS: what you send and see there is not encrypted.
        <button class="link" onclick={() => (httpWarning = null)}>Stay here</button>
        <button class="link danger" onclick={allowHttp}>Continue anyway</button>
      </div>
    {:else if showSettings}
      <label class="settings">Search:
        <select onchange={(e) => (searchDraft = (e.currentTarget as HTMLSelectElement).value || searchDraft)}>
          {#each PRESETS as p (p.template)}<option value={p.template} selected={search === p.template}>{p.label}</option>{/each}
          <option value="" selected={!PRESETS.some((p) => p.template === search)}>Custom (e.g. SearXNG)</option>
        </select>
        <input class="template" aria-label="Search address template" bind:value={searchDraft} placeholder="https://searx.example.org/search?q=&#123;q&#125;" />
        <button class="link" onclick={() => saveSearch(searchDraft)}>Save</button>
        {#if error}<span class="err">{error}</span>{/if}
      </label>
    {:else}
      <nav class="bookmarks" aria-label="Free book sites">
        {#each BOOKMARKS as b (b.url)}<button class="link" onclick={() => go(b.url)}>{b.label}</button>{/each}
      </nav>
      <span class="status" aria-live="polite">
        {#if error}<span class="err">{error}</span>
        {:else if download}<span class="dl {download.status}">{download.status === "started" ? `Downloading ${download.name}…` : download.message}</span>
        {:else}Privacy-hardened: HTTPS-only · trackers blocked · private session{host ? ` · ${host}` : ""}{/if}
      </span>
    {/if}
  </div>
</div>

<style>
  :global(:root) {
    color-scheme: light dark;
    --bg: #f6f5f2;
    --fg: #1f1f1f;
    --muted: #6b6b6b;
    --field: #ffffff;
    --border: rgba(0, 0, 0, 0.14);
    --accent: #2a5db0;
    font-family: system-ui, -apple-system, "Segoe UI", Roboto, "Noto Sans", Ubuntu, Cantarell, sans-serif;
    font-size: 13px;
  }
  @media (prefers-color-scheme: dark) {
    :global(:root) {
      --bg: #26272b;
      --fg: #e6e4df;
      --muted: #a3a19c;
      --field: #1d1e21;
      --border: rgba(255, 255, 255, 0.16);
      --accent: #8ab4f8;
    }
  }
  :global(html, body) {
    margin: 0;
    height: 100%;
    overflow: hidden;
    background: var(--bg);
    color: var(--fg);
  }
  .bar {
    height: 96px;
    box-sizing: border-box;
    padding: 8px 10px 6px;
    display: flex;
    flex-direction: column;
    gap: 6px;
    border-bottom: 1px solid var(--border);
  }
  .row {
    display: flex;
    align-items: center;
    gap: 6px;
    min-height: 36px;
  }
  .second {
    min-height: 32px;
    gap: 12px;
  }
  .btn {
    width: 34px;
    height: 34px;
    border: 0;
    border-radius: 8px;
    background: transparent;
    color: inherit;
    font-size: 18px;
    cursor: pointer;
  }
  .btn:hover,
  .btn.on {
    background: var(--border);
  }
  .address {
    flex: 1;
    height: 34px;
    padding: 0 12px;
    border: 1px solid var(--border);
    border-radius: 17px;
    background: var(--field);
    color: inherit;
    font: inherit;
    font-size: 14px;
  }
  .address:focus {
    outline: 2px solid var(--accent);
  }
  .lock {
    font-size: 13px;
    white-space: nowrap;
  }
  .lock.insecure {
    color: #b3261e;
    font-weight: 600;
  }
  .shield {
    white-space: nowrap;
    color: var(--muted);
  }
  .bookmarks {
    display: flex;
    gap: 4px;
    flex-wrap: nowrap;
    overflow: hidden;
  }
  .link {
    border: 0;
    background: none;
    color: var(--accent);
    font: inherit;
    padding: 4px 8px;
    border-radius: 6px;
    cursor: pointer;
    white-space: nowrap;
  }
  .link:hover {
    background: var(--border);
  }
  .link.danger {
    color: #b3261e;
  }
  .status {
    margin-left: auto;
    color: var(--muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .err,
  .dl.failed,
  .dl.rejected {
    color: #b3261e;
  }
  .dl.imported {
    color: #2e7d32;
    font-weight: 600;
  }
  .warning {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 10px;
    border-radius: 8px;
    background: rgba(179, 38, 30, 0.12);
  }
  .settings {
    display: flex;
    align-items: center;
    gap: 8px;
    flex: 1;
  }
  .template {
    flex: 1;
    height: 28px;
    padding: 0 8px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--field);
    color: inherit;
    font: inherit;
  }
  select {
    height: 28px;
    font: inherit;
    color: inherit;
    background: var(--field);
    border: 1px solid var(--border);
    border-radius: 6px;
  }
</style>

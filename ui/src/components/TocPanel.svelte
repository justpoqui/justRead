<!-- Copyright 2026 The justRead Authors. SPDX-License-Identifier: Apache-2.0 -->
<script lang="ts">
  import type { TocEntry } from "../lib/ipc/types";
  import Self from "./TocPanel.svelte";

  let {
    entries,
    current,
    onselect,
    depth = 0,
  }: { entries: TocEntry[]; current: string | null; onselect: (href: string) => void; depth?: number } = $props();

  const path = (h: string | null) => (h ? h.split("#")[0] : null);
</script>

<ul class:root={depth === 0}>
  {#each entries as e, i (i)}
    <li>
      {#if e.href}
        <button class:current={path(e.href) === current} style="padding-left:{12 + depth * 14}px" onclick={() => onselect(e.href!)}>
          {e.title}
        </button>
      {:else}
        <span class="group" style="padding-left:{12 + depth * 14}px">{e.title}</span>
      {/if}
      {#if e.children?.length}
        <Self entries={e.children} {current} {onselect} depth={depth + 1} />
      {/if}
    </li>
  {/each}
</ul>

<style>
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  ul.root {
    padding: 6px 0 24px;
  }
  button,
  .group {
    display: block;
    width: 100%;
    text-align: left;
    border: 0;
    background: none;
    padding: 7px 14px;
    line-height: 1.3;
    cursor: pointer;
  }
  button:hover {
    background: var(--panel);
  }
  button.current {
    color: var(--accent);
    font-weight: 600;
  }
  .group {
    color: var(--muted);
    cursor: default;
  }
</style>

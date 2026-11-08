<!-- Copyright 2026 The justRead Authors. SPDX-License-Identifier: Apache-2.0 -->
<script lang="ts">
  import { packageUrl } from "../lib/ipc/api";
  import type { BookSummary } from "../lib/ipc/types";

  let { book, small = false }: { book: BookSummary; small?: boolean } = $props();
  let failed = $state(false);

  // A stable hue per title gives covers without art a distinct look.
  const hue = $derived([...book.title].reduce((h, c) => (h * 31 + c.charCodeAt(0)) % 360, 7));
</script>

<div class="cover" class:small style="--hue:{hue}">
  {#if book.cover && !failed}
    <img src={packageUrl(book.id, book.cover)} alt="" loading="lazy" onerror={() => (failed = true)} />
  {:else if !small}
    <div class="generated">
      <span class="t">{book.title}</span>
      <span class="a">{book.authors[0] ?? ""}</span>
    </div>
  {/if}
</div>

<style>
  .cover {
    aspect-ratio: 2 / 3;
    width: 100%;
    border-radius: 6px;
    overflow: hidden;
    background: hsl(var(--hue) 32% 38%);
    box-shadow: 0 4px 14px rgba(0, 0, 0, 0.16);
    transition: transform 0.15s, box-shadow 0.15s;
  }
  .cover.small {
    width: 32px;
    border-radius: 3px;
    box-shadow: none;
  }
  img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
  }
  .generated {
    height: 100%;
    display: flex;
    flex-direction: column;
    justify-content: space-between;
    padding: 14px 12px;
    color: #fff;
    background: linear-gradient(160deg, hsl(var(--hue) 40% 46%), hsl(calc(var(--hue) + 30) 35% 28%));
  }
  .t {
    font-family: Georgia, "Noto Serif", serif;
    font-size: 17px;
    line-height: 1.2;
    font-weight: 600;
    overflow: hidden;
  }
  .a {
    font-size: 12px;
    opacity: 0.85;
  }
</style>

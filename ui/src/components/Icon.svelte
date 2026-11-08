<!-- Copyright 2026 The justRead Authors. SPDX-License-Identifier: Apache-2.0 -->
<!-- Minimal hand-drawn stroke icons (24×24 grid), original to justRead. -->
<script lang="ts">
  let { name, size = 20, filled = false }: { name: string; size?: number; filled?: boolean } = $props();

  const PATHS: Record<string, string> = {
    back: "M15 5l-7 7 7 7",
    left: "M15 5l-7 7 7 7",
    right: "M9 5l7 7-7 7",
    toc: "M8 6h12M8 12h12M8 18h12M4 6h.01M4 12h.01M4 18h.01",
    bookmark: "M6 3h12v18l-6-4-6 4z",
    highlight: "M4 20h16M7 16l9.5-9.5a2.1 2.1 0 0 0-3-3L4 13v3z",
    type: "M4 19l5-14 5 14M5.8 14h6.4M15 19l3-8 3 8M15.9 16.5h4.2",
    plus: "M12 5v14M5 12h14",
    search: "M11 18a7 7 0 1 1 0-14 7 7 0 0 1 0 14zM20 20l-4-4",
    grid: "M4 4h7v7H4zM13 4h7v7h-7zM4 13h7v7H4zM13 13h7v7h-7z",
    list: "M4 6h16M4 12h16M4 18h16",
    trash: "M4 7h16M9 7V4h6v3M6 7l1 13h10l1-13",
    x: "M6 6l12 12M18 6L6 18",
    note: "M5 4h14v12l-4 4H5zM15 20v-4h4",
    copy: "M9 9h11v11H9zM5 15V4h11",
    scroll: "M7 4h10M7 20h10M12 7v10M9 10l3-3 3 3M9 14l3 3 3-3",
    pages: "M3 5h8v14H3zM13 5h8v14h-8z",
    page: "M7 4h10v16H7z",
    sort: "M7 4v16M4 17l3 3 3-3M17 20V4M14 7l3-3 3 3",
    book: "M4 5c3-1 5-1 8 1 3-2 5-2 8-1v14c-3-1-5-1-8 1-3-2-5-2-8-1zM12 6v14",
  };
</script>

<svg
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill={filled ? "currentColor" : "none"}
  stroke="currentColor"
  stroke-width="1.8"
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"
>
  <path d={PATHS[name] ?? ""} />
</svg>

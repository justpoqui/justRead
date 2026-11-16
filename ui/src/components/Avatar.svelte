<!-- Copyright 2026 The justRead Authors. SPDX-License-Identifier: Apache-2.0 -->
<!-- The profile avatar: an emoji, or the name's initials, on the chosen color. -->
<script lang="ts">
  import Icon from "./Icon.svelte";
  import { initials } from "../lib/profile.svelte";

  let { name, emoji, color, size = 36 }: { name: string; emoji: string | null; color: string; size?: number } = $props();
  const letters = $derived(initials(name));
</script>

<span class="avatar" style="width:{size}px;height:{size}px;background:{color};font-size:{Math.round(size * (emoji ? 0.55 : 0.4))}px" aria-hidden="true">
  {#if emoji}{emoji}{:else if letters}{letters}{:else}<Icon name="book" size={Math.round(size * 0.5)} />{/if}
</span>

<style>
  .avatar {
    display: inline-grid;
    place-items: center;
    flex: none;
    border-radius: 50%;
    color: #fff;
    font-weight: 650;
    letter-spacing: 0.02em;
    user-select: none;
  }
</style>

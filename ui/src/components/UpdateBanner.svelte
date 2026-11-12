<!-- Copyright 2026 The justRead Authors. SPDX-License-Identifier: Apache-2.0 -->
<!-- "Update available" in the library (Android). -->
<script lang="ts">
  import Icon from "./Icon.svelte";
  import { updates, install } from "../lib/updates.svelte";
  import { toast } from "../lib/stores/toasts.svelte";

  async function update() {
    const r = await install();
    if (r === "permission") toast("Allow justRead to install updates on the screen that opened, then tap Update again.", "info", 9000);
    else if (r === "error") toast(updates.error ?? "Update failed", "error", 8000);
  }
</script>

{#if updates.info?.available && !updates.dismissed}
  {@const v = updates.info.available}
  <div class="banner" role="status">
    <Icon name="save" size={18} />
    <span class="text">justRead <strong>{v.version}</strong> is available <span class="muted">(you have {updates.info.current})</span></span>
    <button class="primary" disabled={updates.installing} onclick={update}>{updates.installing ? "Downloading…" : "Update"}</button>
    <button disabled={updates.installing} onclick={() => (updates.dismissed = true)}>Later</button>
  </div>
{/if}

<style>
  .banner {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 8px 10px;
    margin: 0 0 16px;
    padding: 10px 12px;
    border: 1px solid var(--border);
    border-radius: 12px;
    background: var(--panel);
  }
  .text {
    flex: 1;
    min-width: 160px;
  }
  .muted {
    color: var(--muted);
    font-size: 12.5px;
  }
  button {
    border: 1px solid var(--border);
    border-radius: 8px;
    background: none;
    padding: 6px 12px;
    cursor: pointer;
  }
  .primary {
    background: var(--accent);
    border-color: var(--accent);
    color: #fff;
  }
</style>

<!-- Copyright 2026 The justRead Authors. SPDX-License-Identifier: Apache-2.0 -->
<script lang="ts">
  import { toasts, dismiss } from "../lib/stores/toasts.svelte";
</script>

<div class="toasts" role="status" aria-live="polite">
  {#each toasts as t (t.id)}
    <button class="toast" class:error={t.kind === "error"} onclick={() => dismiss(t.id)}>{t.text}</button>
  {/each}
</div>

<style>
  .toasts {
    position: fixed;
    bottom: 20px;
    left: 50%;
    transform: translateX(-50%);
    display: flex;
    flex-direction: column;
    gap: 8px;
    z-index: 100;
    max-width: min(560px, calc(100vw - 32px));
  }
  .toast {
    background: var(--fg);
    color: var(--bg);
    border: 0;
    border-radius: 10px;
    padding: 10px 16px;
    font: inherit;
    text-align: left;
    box-shadow: 0 6px 24px rgba(0, 0, 0, 0.2);
    cursor: pointer;
  }
  .toast.error {
    background: #b3261e;
    color: #fff;
  }
</style>

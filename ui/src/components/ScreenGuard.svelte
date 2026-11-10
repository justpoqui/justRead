<!-- Copyright 2026 The justRead Authors. SPDX-License-Identifier: Apache-2.0 -->
<!--
  The reader's own screen timeout (see lib/screen/idle.ts): keeps the screen
  on while reading, dims after `dimAfter` seconds without activity, then
  soft-locks `lockAfter` seconds later until the screen is double-tapped.

  Android: keep-on and dimming are native (window flag + backlight) through
  our plugin. Elsewhere: the web Wake Lock API and a dark overlay.
-->
<script lang="ts">
  import { onMount } from "svelte";
  import Icon from "./Icon.svelte";
  import { api } from "../lib/ipc/api";
  import { IdleGuard, doubleTapDetector, type ScreenState } from "../lib/screen/idle";

  let { enabled, dimAfter, lockAfter }: { enabled: boolean; dimAfter: number; lockAfter: number } = $props();

  const android = typeof navigator !== "undefined" && /Android/i.test(navigator.userAgent);
  let phase = $state<ScreenState>("awake");
  /** Keep the overlay until the finger lifts, so the rest of a waking/unlocking tap can't reach the book. */
  let catching = $state(false);
  // Real timings are applied by the effect below (and follow settings changes).
  const guard = new IdleGuard({ dimAfter: 120, lockAfter: 30 }, (s) => (phase = s));
  const isDoubleTap = doubleTapDetector();

  /** Reader activity (taps, keys, page turns). True = swallow it (it only woke the screen). */
  export function activity(): boolean {
    return enabled ? guard.activity() : false;
  }

  $effect(() => guard.setOptions({ dimAfter, lockAfter }));

  /** Android back / Esc: while dim or locked it must not leave the book. True = handled. */
  export function back(): boolean {
    if (!enabled || phase === "awake") return false;
    if (phase === "dim") guard.activity();
    return true;
  }

  /**
   * Keys count as activity; while dim/locked they are swallowed (true).
   * Also called by the reader for keys pressed inside the book page.
   */
  export function key(e: KeyboardEvent): boolean {
    if (!enabled) return false;
    if (phase === "locked" || phase === "released") {
      // Keyboard: Enter or Space twice quickly unlocks.
      if ((e.key === "Enter" || e.key === " ") && isDoubleTap()) guard.unlock();
      return true;
    }
    return guard.activity();
  }

  // STEP 1: apply the state to the device.
  let wakeLock: { release(): Promise<void> } | null = null;
  async function apply(keepOn: boolean, dim: boolean) {
    if (android) {
      await api.setScreen(keepOn, dim).catch(() => {});
      return;
    }
    // Web Wake Lock (WebView2 / WKWebView); silently unavailable elsewhere.
    try {
      if (keepOn && !wakeLock && document.visibilityState === "visible") {
        wakeLock = (await (navigator as unknown as { wakeLock?: { request(t: "screen"): Promise<{ release(): Promise<void> }> } }).wakeLock?.request("screen")) ?? null;
      } else if (!keepOn && wakeLock) {
        await wakeLock.release();
        wakeLock = null;
      }
    } catch {
      wakeLock = null;
    }
  }
  $effect(() => {
    const on = enabled && phase !== "released";
    void apply(on, enabled && (phase === "dim" || phase === "locked"));
  });
  // Turning the feature off clears any dim/lock.
  $effect(() => {
    if (!enabled && phase !== "awake") guard.unlock();
  });

  onMount(() => {
    const timer = setInterval(() => enabled && guard.tick(), 1000);
    // STEP 2: keys count as activity; while dim/locked they are swallowed.
    const onKey = (e: KeyboardEvent) => {
      if (key(e)) {
        e.preventDefault();
        e.stopImmediatePropagation();
      }
    };
    const onPointer = () => enabled && phase === "awake" && guard.activity();
    // Coming back after the device slept: its own lock already guarded the book.
    const onVisible = () => {
      if (document.visibilityState !== "visible") return;
      if (phase === "released") guard.unlock();
      else wakeLock = null; // the browser drops wake locks when hidden
      void apply(enabled && guard.current !== "released", false);
    };
    window.addEventListener("keydown", onKey, true);
    window.addEventListener("pointerdown", onPointer, true);
    window.addEventListener("wheel", onPointer, { capture: true, passive: true });
    document.addEventListener("visibilitychange", onVisible);
    return () => {
      clearInterval(timer);
      window.removeEventListener("keydown", onKey, true);
      window.removeEventListener("pointerdown", onPointer, true);
      window.removeEventListener("wheel", onPointer, true);
      document.removeEventListener("visibilitychange", onVisible);
      // Leaving the book: the device's own settings apply again.
      void apply(false, false);
    };
  });

  function onOverlayPointer(e: PointerEvent) {
    e.preventDefault();
    e.stopPropagation();
    if (phase === "dim") {
      catching = true;
      guard.activity();
    } else if (isDoubleTap()) {
      catching = true;
      guard.unlock();
    }
  }
  function onOverlayRelease(e: PointerEvent) {
    e.preventDefault();
    e.stopPropagation();
    // A moment longer, so the synthetic click after the touch is caught too.
    setTimeout(() => (catching = false), 120);
  }
</script>

{#if enabled && phase === "dim"}
  <!-- Catches the first touch so it only brightens the screen. -->
  <div class="guard dim" class:shade={!android} role="presentation" onpointerdown={onOverlayPointer}></div>
{:else if enabled && catching}
  <!-- The rest of the tap that woke or unlocked the screen. -->
  <div class="guard catch" role="presentation" onpointerup={onOverlayRelease} onpointercancel={onOverlayRelease} onclick={(e) => e.stopPropagation()}></div>
{:else if enabled && (phase === "locked" || phase === "released")}
  <div class="guard locked" role="dialog" tabindex="-1" aria-modal="true" aria-label="Screen locked" onpointerdown={onOverlayPointer}>
    <div class="msg">
      <Icon name="lock" size={34} />
      <span>Double-tap to unlock</span>
    </div>
  </div>
{/if}

<style>
  .guard {
    position: fixed;
    inset: 0;
    z-index: 100;
    touch-action: none;
    user-select: none;
    -webkit-user-select: none;
  }
  .dim.shade {
    background: rgba(0, 0, 0, 0.55);
  }
  .locked {
    background: rgba(0, 0, 0, 0.94);
    color: rgba(255, 255, 255, 0.85);
    display: grid;
    place-items: center;
  }
  .msg {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 10px;
    font-size: 15px;
    padding: 18px 26px;
    border-radius: 16px;
    background: rgba(255, 255, 255, 0.08);
  }
</style>

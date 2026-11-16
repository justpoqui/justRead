<!-- Copyright 2026 The justRead Authors. SPDX-License-Identifier: Apache-2.0 -->
<script lang="ts">
  import { onMount } from "svelte";
  import Library from "./views/Library.svelte";
  import Reader from "./views/Reader.svelte";
  import Stats from "./views/Stats.svelte";
  import Profile from "./views/Profile.svelte";
  import { loadProfile } from "./lib/profile.svelte";
  import Toasts from "./components/Toasts.svelte";
  import { settings, loadSettings } from "./lib/settings/store.svelte";
  import { themeColors } from "./lib/settings/reader-settings";
  import { installBackButton } from "./lib/back";
  import { autoCheck } from "./lib/updates.svelte";

  // STEP 1: a two-screen app — the library and the reader.
  type Route = { name: "library" } | { name: "reader"; id: string } | { name: "stats" } | { name: "profile" };
  let route: Route = $state({ name: "library" });
  let ready = $state(false);

  onMount(async () => {
    await loadSettings();
    await loadProfile();
    void installBackButton();
    void autoCheck(); // Android, about once a day, if enabled (Settings → Updates)
    ready = true;
  });

  // STEP 2: the app chrome follows the reading theme.
  $effect(() => {
    const c = themeColors(settings);
    const root = document.documentElement.style;
    root.setProperty("--bg", c.background);
    root.setProperty("--fg", c.text);
    root.setProperty("--accent", c.link);
    root.setProperty("--panel", c.forceColors ? "rgba(255,255,255,0.07)" : "rgba(0,0,0,0.045)");
    root.setProperty("--border", c.forceColors ? "rgba(255,255,255,0.14)" : "rgba(0,0,0,0.12)");
    root.setProperty("--muted", c.forceColors ? "rgba(255,255,255,0.6)" : "rgba(0,0,0,0.55)");
    document.documentElement.style.colorScheme = c.forceColors ? "dark" : "light";
  });
</script>

{#if ready}
  {#if route.name === "library"}
    <Library onopen={(id) => (route = { name: "reader", id })} onstats={() => (route = { name: "stats" })} onprofile={() => (route = { name: "profile" })} />
  {:else if route.name === "profile"}
    <Profile onclose={() => (route = { name: "library" })} onstats={() => (route = { name: "stats" })} onopen={(id) => (route = { name: "reader", id })} />
  {:else if route.name === "stats"}
    <Stats onclose={() => (route = { name: "library" })} />
  {:else}
    {#key route.id}
      <Reader id={route.id} onclose={() => (route = { name: "library" })} />
    {/key}
  {/if}
{/if}
<Toasts />

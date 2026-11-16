<!-- Copyright 2026 The justRead Authors. SPDX-License-Identifier: Apache-2.0 -->
<!--
  Profile: the reader's name and avatar, reading goals with progress, and a
  summary of their reading. Everything stays on this device (no accounts).
-->
<script lang="ts">
  import { onMount } from "svelte";
  import Icon from "../components/Icon.svelte";
  import Avatar from "../components/Avatar.svelte";
  import BookCover from "../components/BookCover.svelte";
  import { api } from "../lib/ipc/api";
  import type { BookSummary, ReadingStats } from "../lib/ipc/types";
  import { AVATAR_COLORS, AVATAR_EMOJI, DAILY_GOALS, profile, saveProfile } from "../lib/profile.svelte";
  import { formatDuration } from "../lib/stats/format";
  import { localDay } from "../lib/stats/tracker";
  import { onBack } from "../lib/back";
  import { toast } from "../lib/stores/toasts.svelte";

  let { onclose, onstats, onopen }: { onclose: () => void; onstats: () => void; onopen: (id: string) => void } = $props();

  let stats = $state<ReadingStats | null>(null);
  let finished = $state<BookSummary[]>([]);
  let editing = $state(false);
  const year = new Date().getFullYear();

  onMount(() => {
    // STEP 1: numbers for the goals and the summary.
    api.readingStats(localDay(Date.now()), 7).then((s) => (stats = s)).catch((e) => toast(String(e), "error"));
    api
      .listBooks({ status: "finished", sort: "recent", descending: true })
      .then((list) => (finished = list.sort((a, b) => (b.finishedAt ?? 0) - (a.finishedAt ?? 0)).slice(0, 8)))
      .catch(() => {});
    return onBack(() => (editing ? (editing = false) : onclose(), true));
  });

  const pct = (n: number, of: number) => (of > 0 ? Math.min(100, Math.round((n / of) * 100)) : 0);
  const todayMin = $derived(Math.floor((stats?.todaySeconds ?? 0) / 60));
</script>

<div class="profile-view">
  <header>
    <button class="icon-btn" title="Back to library" onclick={onclose}><Icon name="back" /></button>
    <h1>Profile</h1>
  </header>

  <main>
    <!-- STEP 2: who. -->
    <section class="who">
      <Avatar name={profile.name} emoji={profile.emoji} color={profile.color} size={72} />
      <div class="who-text">
        <h2>{profile.name || "Reader"}</h2>
        <span class="muted">{stats ? `${stats.booksFinished} finished · ${stats.booksReading} reading` : ""}</span>
      </div>
      <button class="btn" onclick={() => (editing = !editing)}>{editing ? "Done" : "Edit"}</button>
    </section>

    {#if editing}
      <section class="edit" aria-label="Edit profile">
        <label class="field">Name
          <input type="text" maxlength="40" placeholder="Your name" bind:value={profile.name} onchange={saveProfile} />
        </label>
        <span class="label">Color</span>
        <div class="swatches" role="radiogroup" aria-label="Avatar color">
          {#each AVATAR_COLORS as c (c)}
            <button role="radio" aria-checked={profile.color === c} aria-label="Color {c}" class="swatch" class:on={profile.color === c} style="background:{c}"
              onclick={() => ((profile.color = c), saveProfile())}></button>
          {/each}
        </div>
        <span class="label">Picture</span>
        <div class="emoji" role="radiogroup" aria-label="Avatar picture">
          <button role="radio" aria-checked={profile.emoji === null} class:on={profile.emoji === null} onclick={() => ((profile.emoji = null), saveProfile())}>Initials</button>
          {#each AVATAR_EMOJI as e (e)}
            <button role="radio" aria-checked={profile.emoji === e} class:on={profile.emoji === e} onclick={() => ((profile.emoji = e), saveProfile())}>{e}</button>
          {/each}
        </div>
      </section>
    {/if}

    <!-- STEP 3: goals. -->
    <section class="goals">
      <h3>Goals</h3>
      <div class="goal">
        <div class="goal-head">
          <span>Books in {year}</span>
          <span class="muted">{stats?.finishedThisYear ?? 0}{profile.yearGoal ? ` of ${profile.yearGoal}` : ""}</span>
        </div>
        {#if profile.yearGoal}
          <div class="bar" role="progressbar" aria-label="Yearly goal" aria-valuenow={pct(stats?.finishedThisYear ?? 0, profile.yearGoal)} aria-valuemin="0" aria-valuemax="100">
            <span style="width:{pct(stats?.finishedThisYear ?? 0, profile.yearGoal)}%"></span>
          </div>
        {/if}
        <label class="field inline">Goal
          <input type="number" min="0" max="365" step="1" aria-label="Books to finish this year" bind:value={profile.yearGoal} onchange={saveProfile} />
          <span class="muted">books (0 = none)</span>
        </label>
      </div>
      <div class="goal">
        <div class="goal-head">
          <span>Reading today</span>
          <span class="muted">{todayMin} min{profile.dailyGoal ? ` of ${profile.dailyGoal}` : ""}</span>
        </div>
        {#if profile.dailyGoal}
          <div class="bar" role="progressbar" aria-label="Daily goal" aria-valuenow={pct(todayMin, profile.dailyGoal)} aria-valuemin="0" aria-valuemax="100">
            <span style="width:{pct(todayMin, profile.dailyGoal)}%"></span>
          </div>
        {/if}
        <label class="field inline">Goal
          <select aria-label="Minutes to read each day" bind:value={profile.dailyGoal} onchange={saveProfile}>
            {#each DAILY_GOALS as m (m)}<option value={m}>{m === 0 ? "None" : `${m} minutes a day`}</option>{/each}
          </select>
        </label>
      </div>
    </section>

    <!-- STEP 4: reading summary. -->
    {#if stats}
      <section class="summary">
        <h3>Your reading</h3>
        <dl>
          <div><dt>Reading streak</dt><dd>{stats.streakDays} {stats.streakDays === 1 ? "day" : "days"}</dd></div>
          <div><dt>Last 7 days</dt><dd>{formatDuration(stats.weekSeconds)}</dd></div>
          <div><dt>All time</dt><dd>{formatDuration(stats.totalSeconds)}</dd></div>
          <div><dt>Books finished</dt><dd>{stats.booksFinished}</dd></div>
        </dl>
        <button class="btn" onclick={onstats}><Icon name="chart" size={16} /> See all stats</button>
      </section>
    {/if}

    {#if finished.length}
      <section class="finished">
        <h3>Recently finished</h3>
        <ul>
          {#each finished as b (b.id)}
            <li><button class="fin" title={b.title} onclick={() => onopen(b.id)}><BookCover book={b} /><span>{b.title}</span></button></li>
          {/each}
        </ul>
      </section>
    {/if}

    <p class="note">Your profile and reading stay on this device. There are no accounts.</p>
  </main>
</div>

<style>
  .profile-view {
    height: 100%;
    display: flex;
    flex-direction: column;
  }
  header {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 14px;
    border-bottom: 1px solid var(--border);
  }
  h1 {
    font-size: 16px;
    margin: 0;
  }
  main {
    flex: 1;
    overflow: auto;
    padding: 18px 20px 32px;
    max-width: 640px;
    width: 100%;
    margin: 0 auto;
    box-sizing: border-box;
    display: flex;
    flex-direction: column;
    gap: 22px;
  }
  .who {
    display: flex;
    align-items: center;
    gap: 14px;
  }
  .who-text {
    flex: 1;
    min-width: 0;
  }
  h2 {
    margin: 0 0 2px;
    font-size: 20px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  h3 {
    margin: 0 0 10px;
    font-size: 12px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--muted);
  }
  .muted,
  .note {
    color: var(--muted);
    font-size: 13px;
  }
  .btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: none;
    padding: 6px 14px;
    cursor: pointer;
  }
  .edit {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px 14px;
    border: 1px solid var(--border);
    border-radius: 12px;
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .field.inline {
    flex-direction: row;
    align-items: center;
    gap: 8px;
    margin-top: 6px;
  }
  .field.inline input {
    width: 72px;
  }
  .label {
    font-size: 13px;
    margin-top: 4px;
  }
  .swatches,
  .emoji {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  .swatch {
    width: 32px;
    height: 32px;
    border-radius: 50%;
    border: 0;
    cursor: pointer;
  }
  .swatch.on,
  .emoji button.on {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
  .emoji button {
    min-width: 40px;
    height: 40px;
    border: 1px solid var(--border);
    border-radius: 10px;
    background: none;
    font-size: 20px;
    cursor: pointer;
  }
  .emoji button:first-child {
    font-size: 13px;
    padding: 0 10px;
  }
  .goal {
    padding: 12px 14px;
    border: 1px solid var(--border);
    border-radius: 12px;
    margin-bottom: 10px;
  }
  .goal-head {
    display: flex;
    justify-content: space-between;
    margin-bottom: 8px;
    font-weight: 600;
  }
  .bar {
    height: 8px;
    border-radius: 4px;
    background: var(--panel);
    overflow: hidden;
  }
  .bar span {
    display: block;
    height: 100%;
    border-radius: 4px;
    background: var(--accent);
  }
  dl {
    margin: 0 0 12px;
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(130px, 1fr));
    gap: 10px;
  }
  dl div {
    padding: 10px 12px;
    border: 1px solid var(--border);
    border-radius: 10px;
  }
  dt {
    color: var(--muted);
    font-size: 12.5px;
  }
  dd {
    margin: 2px 0 0;
    font-size: 18px;
    font-weight: 650;
    font-variant-numeric: tabular-nums;
  }
  .finished ul {
    list-style: none;
    margin: 0;
    padding: 0 0 6px;
    display: flex;
    gap: 14px;
    overflow-x: auto;
  }
  .finished li {
    flex: none;
    width: 90px;
  }
  .fin {
    width: 100%;
    display: flex;
    flex-direction: column;
    gap: 4px;
    border: 0;
    background: none;
    padding: 0;
    text-align: left;
    cursor: pointer;
    font-size: 12px;
  }
  .fin span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>

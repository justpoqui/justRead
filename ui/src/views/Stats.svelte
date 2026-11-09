<!-- Copyright 2026 The justRead Authors. SPDX-License-Identifier: Apache-2.0 -->
<!--
  Reading stats (Phase 8). Headline numbers as tiles, then one chart:
  reading time per day for the last 14 days (single series, one hue, so no
  legend; hover/tap a bar for its value; a table view for screen readers and
  anyone who prefers numbers). Everything comes from the local database.
-->
<script lang="ts">
  import { onMount } from "svelte";
  import Icon from "../components/Icon.svelte";
  import { api } from "../lib/ipc/api";
  import type { ReadingStats } from "../lib/ipc/types";
  import { formatDuration, shortDuration } from "../lib/stats/format";
  import { localDay } from "../lib/stats/tracker";
  import { onBack } from "../lib/back";
  import { toast } from "../lib/stores/toasts.svelte";

  let { onclose }: { onclose: () => void } = $props();
  let stats = $state<ReadingStats | null>(null);
  let hover = $state<number | null>(null);
  const DAYS = 14;

  onMount(() => {
    api
      .readingStats(localDay(Date.now()), DAYS)
      .then((s) => (stats = s))
      .catch((e) => toast(String(e), "error"));
    return onBack(() => (onclose(), true));
  });

  // STEP 1: chart geometry (SVG user units; the SVG scales to the width).
  const W = 560;
  const H = 180;
  const TOP = 22; // room for the direct label on the longest day
  const BASE = H - 24; // baseline; day labels below
  const GAP = 2; // surface gap between bars
  const max = $derived(Math.max(60, ...(stats?.daily.map((d) => d.seconds) ?? [0])));
  const slot = $derived(W / DAYS);
  const barW = $derived(Math.min(28, slot - GAP));
  const peak = $derived(stats ? stats.daily.reduce((best, d, i, a) => (d.seconds > a[best]!.seconds ? i : best), 0) : -1);

  function barPath(i: number, seconds: number): string {
    // Rounded (4px) top, square bottom anchored to the baseline.
    const h = Math.max(seconds > 0 ? 3 : 0, ((BASE - TOP) * seconds) / max);
    if (h === 0) return "";
    const x = i * slot + (slot - barW) / 2;
    const y = BASE - h;
    const r = Math.min(4, h, barW / 2);
    return `M${x},${BASE}V${y + r}Q${x},${y} ${x + r},${y}H${x + barW - r}Q${x + barW},${y} ${x + barW},${y + r}V${BASE}Z`;
  }

  const dayName = (day: string, short = false) => {
    const [y, m, d] = day.split("-").map(Number);
    return new Date(y!, m! - 1, d!).toLocaleDateString(undefined, short ? { weekday: "narrow" } : { weekday: "short", month: "short", day: "numeric" });
  };
</script>

<div class="stats-view">
  <header>
    <button class="icon-btn" title="Back to library" onclick={onclose}><Icon name="back" /></button>
    <h1>Reading stats</h1>
  </header>

  {#if stats}
    <main>
      <!-- STEP 2: headline numbers. -->
      <div class="tiles">
        <div class="tile"><span class="v">{formatDuration(stats.todaySeconds)}</span><span class="k">Today</span></div>
        <div class="tile"><span class="v">{formatDuration(stats.weekSeconds)}</span><span class="k">Last 7 days</span></div>
        <div class="tile"><span class="v">{stats.streakDays} {stats.streakDays === 1 ? "day" : "days"}</span><span class="k">Reading streak</span></div>
        <div class="tile"><span class="v">{stats.finishedThisYear}</span><span class="k">Finished this year</span></div>
      </div>
      <p class="sub">
        {formatDuration(stats.totalSeconds)} read in total · {stats.booksReading} reading · {stats.booksFinished} finished
      </p>

      <!-- STEP 3: the chart. -->
      <section class="chart" aria-labelledby="chart-title">
        <h2 id="chart-title">Reading time, last {DAYS} days</h2>
        <div class="plot">
          <svg viewBox="0 0 {W} {H}" role="img" aria-label="Bar chart of reading time per day; the table below has the values">
            <line x1="0" x2={W} y1={BASE + 0.5} y2={BASE + 0.5} class="axis" />
            {#each stats.daily as d, i (d.day)}
              <g
                class="bar"
                class:dim={hover !== null && hover !== i}
                role="presentation"
                onpointerenter={() => (hover = i)}
                onpointerleave={() => (hover = null)}
                onclick={() => (hover = hover === i ? null : i)}
              >
                <!-- Hit area: the whole column, bigger than the bar. -->
                <rect x={i * slot} y="0" width={slot} height={H} fill="transparent" />
                <path d={barPath(i, d.seconds)} class="mark" />
                {#if i === peak && d.seconds > 0}
                  <text x={i * slot + slot / 2} y={BASE - ((BASE - TOP) * d.seconds) / max - 6} class="label">{shortDuration(d.seconds)}</text>
                {/if}
                <text x={i * slot + slot / 2} y={H - 8} class="day" class:today={i === stats.daily.length - 1}>{dayName(d.day, true)}</text>
              </g>
            {/each}
          </svg>
          {#if hover !== null}
            {@const d = stats.daily[hover]!}
            <div class="tip" style="left:{((hover + 0.5) / DAYS) * 100}%">
              <strong>{formatDuration(d.seconds)}</strong><span>{dayName(d.day)}</span>
            </div>
          {/if}
        </div>
        <details>
          <summary>Show as table</summary>
          <table>
            <thead><tr><th>Day</th><th>Reading time</th></tr></thead>
            <tbody>
              {#each [...stats.daily].reverse() as d (d.day)}
                <tr><td>{dayName(d.day)}</td><td>{formatDuration(d.seconds)}</td></tr>
              {/each}
            </tbody>
          </table>
        </details>
      </section>
      <p class="note">Only active reading counts: time stops after two minutes without turning a page. Stats stay on this device.</p>
    </main>
  {/if}
</div>

<style>
  .stats-view {
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
    max-width: 720px;
    width: 100%;
    margin: 0 auto;
    box-sizing: border-box;
  }
  .tiles {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(140px, 1fr));
    gap: 10px;
  }
  .tile {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 12px 14px;
    border: 1px solid var(--border);
    border-radius: 12px;
  }
  .v {
    font-size: 22px;
    font-weight: 650;
    font-variant-numeric: tabular-nums;
  }
  .k,
  .sub,
  .note {
    color: var(--muted);
    font-size: 13px;
  }
  .sub {
    margin: 10px 0 22px;
  }
  .chart h2 {
    font-size: 14px;
    margin: 0 0 8px;
  }
  .plot {
    position: relative;
  }
  svg {
    width: 100%;
    height: auto;
    display: block;
    overflow: visible;
  }
  .axis {
    stroke: var(--border);
    stroke-width: 1;
  }
  .mark {
    fill: var(--accent);
  }
  .bar {
    cursor: pointer;
  }
  .bar.dim .mark {
    opacity: 0.45;
  }
  .label,
  .day {
    text-anchor: middle;
    font-size: 11px;
    fill: var(--muted);
  }
  .label {
    fill: var(--fg);
    font-weight: 600;
  }
  .day.today {
    fill: var(--fg);
    font-weight: 600;
  }
  .tip {
    position: absolute;
    top: 0;
    transform: translateX(-50%);
    pointer-events: none;
    display: flex;
    flex-direction: column;
    align-items: center;
    padding: 6px 10px;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 8px;
    box-shadow: 0 6px 20px rgba(0, 0, 0, 0.15);
    font-size: 12px;
    white-space: nowrap;
  }
  .tip span {
    color: var(--muted);
  }
  details {
    margin-top: 12px;
    font-size: 13px;
  }
  summary {
    cursor: pointer;
    color: var(--muted);
  }
  table {
    margin-top: 8px;
    border-collapse: collapse;
  }
  td,
  th {
    text-align: left;
    padding: 3px 16px 3px 0;
    font-variant-numeric: tabular-nums;
  }
  th {
    color: var(--muted);
    font-weight: 500;
  }
  .note {
    margin-top: 22px;
  }
</style>

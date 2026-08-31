<script lang="ts">
  import Select from "./Select.svelte";
  import {
    dictationStats,
    formatCount,
    formatDuration,
    type DictationStats,
    type StatsRange,
  } from "$lib/dictation";

  const RANGES: { id: StatsRange; label: string }[] = [
    { id: "today", label: "Today" },
    { id: "week", label: "Last 7 days" },
    { id: "month", label: "Last 30 days" },
    { id: "allTime", label: "All time" },
  ];

  let range = $state<StatsRange>("allTime");
  let stats = $state<DictationStats | null>(null);

  $effect(() => {
    const wanted = range;
    void dictationStats(wanted).then((next) => {
      // Discard a reply that arrived after the reader moved on.
      if (wanted === range) stats = next;
    });
  });

  const saved = $derived(formatDuration(stats?.secondsSaved ?? 0));
  const words = $derived(formatCount(stats?.totalWords ?? 0));
</script>

<div class="head">
  <span class="label">Statistics</span>
  <Select
    value={range}
    options={RANGES.map((option) => ({ value: option.id, label: option.label }))}
    onchange={(next) => (range = next as StatsRange)}
    ariaLabel="Statistics range"
  />
  <span class="spacer"></span>
  {#if stats}
    <span class="count">
      {stats.dictations.toLocaleString()}
      {stats.dictations === 1 ? "dictation" : "dictations"}
    </span>
  {/if}
</div>

<div class="board">
  <div class="stat">
    <span class="name">Words per minute</span>
    <span class="figure">{stats?.wordsPerMinute ?? 0}</span>
  </div>

  <div class="stat">
    <span class="name" title="Against typing the same words at 40 words per minute">
      Time saved
    </span>
    <span class="figure">
      {#each saved as part (part.unit)}
        {part.value}<em>{part.unit}</em>
      {/each}
    </span>
  </div>

  <div class="stat">
    <span class="name">Total words</span>
    <span class="figure">{words.value}<em>{words.unit}</em></span>
  </div>
</div>

<style>
  .head {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: 0 2px var(--space-2);
  }

  .label {
    font-size: var(--text-meta);
    font-weight: var(--weight-strong);
    letter-spacing: 0.14em;
    text-transform: uppercase;
  }

  .spacer {
    flex: 1;
  }

  .count {
    font-size: var(--text-meta);
    color: var(--text-3);
    font-variant-numeric: tabular-nums;
  }

  .board {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    margin-bottom: var(--space-6);
    border-radius: var(--radius-lg);
    background: rgba(255, 255, 255, 0.02);
    overflow: hidden;
  }

  /* A hairline between the three, not around them: one board holding three
     figures rather than three cards that happen to sit in a row. */
  .stat + .stat {
    border-left: 1px solid var(--hairline);
  }

  .stat {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-5) var(--space-4) var(--space-6);
  }

  .name {
    font-size: var(--text-label);
    letter-spacing: 0.06em;
    color: var(--text-3);
  }

  .figure {
    font-size: var(--text-hero);
    font-weight: 300;
    line-height: 1;
    letter-spacing: -0.02em;
    /* Fixed width digits: these tick while you watch, and proportional
       figures make the whole row shuffle when a 1 becomes a 4. */
    font-variant-numeric: tabular-nums;
  }

  /* The unit rides at the baseline, smaller and quieter, so "12h 17m" reads
     as one number rather than four things. */
  em {
    margin: 0 var(--space-1) 0 1px;
    font-size: var(--text-query);
    font-style: normal;
    font-weight: 400;
    color: var(--text-3);
  }

  em:last-child {
    margin-right: 0;
  }
</style>

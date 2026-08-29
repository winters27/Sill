<script lang="ts">
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
  <select bind:value={range} aria-label="Statistics range">
    {#each RANGES as option (option.id)}
      <option value={option.id}>{option.label}</option>
    {/each}
  </select>
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
    gap: 10px;
    padding: 0 2px 10px;
  }

  .label {
    font-size: 12px;
    font-weight: 600;
    letter-spacing: 0.14em;
    text-transform: uppercase;
  }

  select {
    padding: 3px 6px;
    border: 0;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--text-muted);
    font: inherit;
    font-size: 12px;
    outline: none;
    cursor: pointer;
    transition: color 0.15s var(--ease);
  }

  select:hover {
    color: var(--core-foreground);
  }

  select option {
    background: var(--core-secondary-background);
    color: var(--core-foreground);
  }

  .spacer {
    flex: 1;
  }

  .count {
    font-size: 12px;
    color: var(--text-faint);
    font-variant-numeric: tabular-nums;
  }

  .board {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    margin-bottom: 26px;
    border-radius: 10px;
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
    gap: 8px;
    padding: 22px 16px 26px;
  }

  .name {
    font-size: 11px;
    letter-spacing: 0.06em;
    color: var(--text-faint);
  }

  .figure {
    font-size: 40px;
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
    margin: 0 5px 0 1px;
    font-size: 17px;
    font-style: normal;
    font-weight: 400;
    color: var(--text-faint);
  }

  em:last-child {
    margin-right: 0;
  }
</style>

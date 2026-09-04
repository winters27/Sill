<script lang="ts">
  /**
   * What each extension costs, and which one is the expensive one.
   *
   * ## Why this screen exists
   *
   * A launcher that got slower after somebody installed four extensions had no
   * way of saying which of the four did it, and the honest answer to "is this
   * extension expensive" was a shrug. This is that answer.
   *
   * It is deliberately small. The line at the top names one extension; the
   * table under it is the numbers that line was drawn from, for whoever wants
   * to check rather than take it. A dashboard would be a worse version of the
   * same thing, because reading a dashboard is the work this is meant to save.
   *
   * ## Why it takes rows rather than fetching them
   *
   * So it can be drawn against real measurements without an application
   * around it, which is how the numbers in `docs/budgets.md` were checked
   * against what this actually says about them. The panel does the asking.
   */
  import Row from "./Row.svelte";
  import Section from "./Section.svelte";
  import { hint } from "$lib/hint";
  import {
    describeRunning,
    memoryBytes,
    showBytes,
    showMs,
    verdict,
    type CostRow,
  } from "$lib/costs";

  interface Props {
    /** Slowest to open first, as Rust ordered them. */
    rows: CostRow[];
  }

  let { rows }: Props = $props();

  const whichOne = $derived(verdict(rows));
</script>

<Section
  label="What they cost"
  description="Measured while Sill has been running, so an extension you have not opened this time is not here. Opening one is slower the first time, because the runtime that extensions share has to start."
>
  {#if rows.length === 0}
    <!-- not a setting: what to do to make a reading exist, when there is none -->
    <Row
      title="Nothing opened yet"
      description="Open one of these extensions from the launcher and come back. Times are kept until Sill closes."
    />
  {:else}
    <div class="verdict">{whichOne}</div>

    <div class="board">
      <div class="head">
        <span>Extension</span>
        <span
          class="figure"
          use:hint={"From pressing Enter to the extension drawing, when Sill had to start the runtime first"}
        >
          First open
        </span>
        <span class="figure" use:hint={"The same, when the runtime was already up"}>
          After that
        </span>
        <span
          class="figure"
          use:hint={"What it is holding now, or the most it was holding when it was last closed"}
        >
          Memory
        </span>
      </div>

      {#each rows as row (row.extension)}
        <div class="line">
          <span class="who">{row.title}</span>
          <span class="figure">{showMs(row.cost.coldMs)}</span>
          <span class="figure">{showMs(row.cost.warmMs)}</span>
          <span class="figure">{showBytes(memoryBytes(row.cost))}</span>
        </div>

        <!--
          One line per command that is loaded, and only when something is. An
          extension is not one program, and "this one is using 63 MB" is half
          an answer when three of its four commands are asleep.
        -->
        {#each row.cost.running as one (one.session)}
          <div class="doing">{describeRunning(one)}</div>
        {/each}
      {/each}
    </div>
  {/if}
</Section>

<style>
  /*
   * The answer, above the numbers it came from.
   *
   * Somebody opens this with a question rather than to browse, so the sentence
   * goes where their eye lands and the table is there to be checked.
   */
  .verdict {
    padding: var(--space-3) var(--space-4);
    font-size: var(--text-body);
    color: var(--text-1);
  }

  .board {
    padding: 0 var(--space-4) var(--space-3);
  }

  .head,
  .line {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 96px 96px 104px;
    gap: 0 var(--space-2);
    align-items: baseline;
  }

  .head {
    padding-bottom: var(--space-1);
    font-size: var(--text-micro);
    letter-spacing: var(--track-label);
    text-transform: uppercase;
    color: var(--text-3);
    border-bottom: 1px solid var(--hairline);
  }

  .line {
    padding: var(--space-1) 0;
  }

  .who {
    font-size: var(--text-meta);
    color: var(--text-1);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* Fixed width digits, so the three columns line up as columns rather than
     shuffling sideways when a 1 becomes a 4. */
  .figure {
    text-align: right;
    font-size: var(--text-meta);
    color: var(--text-2);
    font-variant-numeric: tabular-nums;
  }

  .head .figure {
    font-size: var(--text-micro);
    color: var(--text-3);
  }

  /* Indented under the row it belongs to, because it is about one of that
     extension's commands rather than about the extension. */
  .doing {
    padding: 0 0 var(--space-1) var(--space-3);
    font-size: var(--text-meta);
    color: var(--text-3);
  }
</style>

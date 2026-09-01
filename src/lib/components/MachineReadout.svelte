<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";

  type Consumer = {
    name: string;
    bytes: number;
    cpu: number;
    path: string | null;
  };

  type Reading = {
    cpu: number;
    memoryUsed: number;
    memoryTotal: number;
    count: number;
    top: Consumer[];
    sill: number;
  };

  let reading = $state<Reading | null>(null);
  let failed = $state("");

  /**
   * How often the machine is asked.
   *
   * A second is what every system monitor settles on, and for a good reason:
   * faster reads as noise rather than as information, because the numbers
   * move more than the thing they describe does. Slower and it stops feeling
   * live.
   *
   * The poll exists only while this is on screen. `onMount` returns the
   * teardown, so closing the view stops it: a launcher that keeps reading the
   * machine after it is dismissed is the exact thing this feature is about.
   */
  const EVERY = 1_000;

  const memoryPercent = $derived(
    reading && reading.memoryTotal > 0
      ? (reading.memoryUsed / reading.memoryTotal) * 100
      : 0,
  );

  function gb(bytes: number): string {
    return `${(bytes / 1_073_741_824).toFixed(1)} GB`;
  }

  function mb(bytes: number): string {
    return `${Math.round(bytes / 1_048_576)} MB`;
  }

  async function take() {
    try {
      reading = await invoke<Reading>("machine_reading");
      failed = "";
    } catch (err) {
      failed = `${err}`;
    }
  }

  onMount(() => {
    void take();
    const timer = setInterval(take, EVERY);

    return () => {
      clearInterval(timer);
      // The next reading is measured against the last one, and the last one
      // must not be from whenever this was closed.
      void invoke("forget_machine_reading");
    };
  });
</script>

<div class="readout">
  {#if failed}
    <p class="failed">{failed}</p>
  {:else if !reading}
    <p class="waiting">Reading the machine…</p>
  {:else}
    <div class="dials">
      <div class="dial">
        <div class="head">
          <span class="what">Processor</span>
          <span class="figure">{reading.cpu.toFixed(0)}%</span>
        </div>
        <div class="track">
          <div class="fill" style:width={`${reading.cpu}%`}></div>
        </div>
      </div>

      <div class="dial">
        <div class="head">
          <span class="what">Memory</span>
          <span class="figure">
            {gb(reading.memoryUsed)} of {gb(reading.memoryTotal)}
          </span>
        </div>
        <div class="track">
          <div class="fill" style:width={`${memoryPercent}%`}></div>
        </div>
      </div>
    </div>

    <ul class="top">
      {#each reading.top as one (one.name + one.bytes)}
        <li class="one">
          <span class="name">{one.name}</span>
          <span class="cost">
            {mb(one.bytes)}{#if one.cpu >= 1}<span class="cpu"
                >&middot; {one.cpu.toFixed(0)}% CPU</span
              >{/if}
          </span>
        </li>
      {/each}
    </ul>

    <!--
      Sill's own weight, last and quiet. The whole project's claim is that it
      idles at almost nothing, and the honest place to say so is underneath
      everything else on the same screen, measured the same way.
    -->
    <p class="mine">
      {reading.count} programs running. Sill is using {mb(reading.sill)}.
    </p>
  {/if}
</div>

<style>
  .readout {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
    padding: var(--space-4);
  }

  .dials {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  .head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    padding-bottom: var(--space-2);
  }

  .what {
    color: var(--text-1);
    font-size: var(--text-body);
    font-weight: var(--weight-medium);
  }

  .figure {
    color: var(--text-2);
    font-size: var(--text-meta);
    font-variant-numeric: tabular-nums;
  }

  /* A bar rather than a graph. A graph needs history to mean anything, and
     history is state this deliberately does not keep. */
  .track {
    height: 6px;
    border-radius: var(--radius-1);
    background: var(--fill-2);
    overflow: hidden;
  }

  .fill {
    height: 100%;
    background: var(--accent);
    /* Matched to the poll, so the bar glides between readings instead of
       stepping once a second. */
    transition: width 1s linear;
  }

  .top {
    display: flex;
    flex-direction: column;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .one {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: var(--space-3);
    padding: var(--space-2) 0;
  }

  .one + .one {
    box-shadow: inset 0 1px 0 var(--hairline);
  }

  .name {
    color: var(--text-1);
    font-size: var(--text-body);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .cost {
    flex: none;
    color: var(--text-2);
    font-size: var(--text-meta);
    font-variant-numeric: tabular-nums;
  }

  .cpu {
    padding-left: var(--space-2);
    color: var(--text-3);
  }

  .mine,
  .waiting,
  .failed {
    margin: 0;
    color: var(--text-3);
    font-size: var(--text-meta);
  }
</style>

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

  interface Props {
    compact?: boolean;
  }

  let { compact = false }: Props = $props();

  let reading = $state<Reading | null>(null);
  let failed = $state("");

  /**
   * How often the machine is asked.
   *
   * A second is what every system monitor settles on, and for a good reason:
   * faster reads as noise rather than as information, because the numbers move
   * more than the thing they describe does. Slower stops feeling live.
   *
   * The poll exists only while this is on screen. `onMount` returns the
   * teardown, so closing the view stops it, which for this feature in
   * particular would be embarrassing to get wrong.
   */
  const EVERY = 1_000;

  /** The ring's geometry. One place, so the arc and the track agree. */
  const R = 34;
  const CIRCUMFERENCE = 2 * Math.PI * R;

  const memoryPercent = $derived(
    reading && reading.memoryTotal > 0
      ? (reading.memoryUsed / reading.memoryTotal) * 100
      : 0,
  );

  /** The heaviest one, so the bars below are drawn against something real. */
  const heaviest = $derived(
    reading?.top.reduce((most, one) => Math.max(most, one.bytes), 0) ?? 0,
  );

  /**
   * Green, amber, red.
   *
   * Not the selection accent, deliberately: that colour means "this is the one
   * you picked" everywhere else in Sill, and a processor dial is not a
   * selection. These are the palette's own semantic colours, doing the one
   * thing a gauge is for, which is saying whether the number is fine.
   */
  function tone(percent: number): string {
    if (percent >= 85) return "var(--accent-red)";
    if (percent >= 60) return "var(--accent-orange)";
    return "var(--accent-green)";
  }

  function gb(bytes: number): string {
    return `${(bytes / 1_073_741_824).toFixed(1)}`;
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

{#snippet ring(percent: number, label: string, figure: string, unit: string)}
  <div class="tile gauge">
    <svg class="dial" viewBox="0 0 80 80" aria-hidden="true">
      <circle class="track" cx="40" cy="40" r={R} />
      <circle
        class="arc"
        cx="40"
        cy="40"
        r={R}
        stroke={tone(percent)}
        stroke-dasharray={CIRCUMFERENCE}
        stroke-dashoffset={CIRCUMFERENCE * (1 - Math.min(percent, 100) / 100)}
      />
    </svg>

    <div class="middle">
      <span class="figure">{figure}</span>
      <span class="unit">{unit}</span>
    </div>

    <span class="label">{label}</span>
  </div>
{/snippet}

{#if compact}
  {#if reading}
    <span class="strip">
      <span class="dot" style:background={tone(reading.cpu)}></span>
      {reading.cpu.toFixed(0)}%
      <span class="apart">{mb(reading.sill)}</span>
    </span>
  {/if}
{:else}
<div class="readout">
  {#if failed}
    <p class="quiet">{failed}</p>
  {:else if !reading}
    <p class="quiet">Reading the machine…</p>
  {:else}
    <div class="gauges">
      {@render ring(reading.cpu, "Processor", reading.cpu.toFixed(0), "%")}
      {@render ring(
        memoryPercent,
        "Memory",
        gb(reading.memoryUsed),
        `of ${gb(reading.memoryTotal)} GB`,
      )}
    </div>

    <div class="tile heaviest">
      <span class="heading">Heaviest right now</span>

      <ul class="programs">
        {#each reading.top as one (one.name + one.bytes)}
          <li class="program">
            <span class="name">{one.name}</span>

            <!-- Drawn against the heaviest rather than against the machine's
                 memory: five programs each at 4% of 32 GB would be five bars
                 too short to compare, and comparing them is the point. -->
            <span class="meter" aria-hidden="true">
              <span
                class="level"
                style:width={`${heaviest > 0 ? (one.bytes / heaviest) * 100 : 0}%`}
              ></span>
            </span>

            <span class="cost">
              {mb(one.bytes)}{#if one.cpu >= 1}<span class="cpu"
                  >{one.cpu.toFixed(0)}%</span
                >{/if}
            </span>
          </li>
        {/each}
      </ul>
    </div>

    <!--
      Sill's own weight, on its own and last. The whole project's claim is that
      it idles at almost nothing, and the honest place to say so is on the same
      screen as everything else, measured the same way.
    -->
    <div class="tile mine">
      <span class="who">Sill</span>
      <span class="weight">{mb(reading.sill)}</span>
      <span class="among">of {reading.count} programs running</span>
    </div>
  {/if}
</div>
{/if}

<style>
  /* The chin: one dot, one number, and what Sill costs. */
  .strip {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    color: var(--text-1);
    font-size: var(--text-meta);
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }

  .dot {
    width: 6px;
    height: 6px;
    border-radius: var(--radius-pill);
  }

  .apart {
    padding-left: var(--space-1);
    color: var(--text-3);
  }

  .readout {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    height: 100%;
    padding: var(--space-4);
  }

  .gauges {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: var(--space-2);
  }

  /*
   * One tile, used four ways. The bevel and the sheen are what make these read
   * as objects sitting on the glass rather than as boxes drawn on it, and they
   * are the same two the rest of Sill's tiles use.
   */
  .tile {
    position: relative;
  }

  .gauge {
    display: grid;
    place-items: center;
  }

  .dial {
    width: 116px;
    height: 116px;
    /* Twelve o'clock, rather than three. */
    transform: rotate(-90deg);
  }

  .track,
  .arc {
    fill: none;
    stroke-width: 6;
  }

  .track {
    stroke: var(--fill-2);
  }

  .arc {
    stroke-linecap: round;
    /* Matched to the poll, so the arc glides between readings rather than
       stepping once a second. */
    transition:
      stroke-dashoffset 1s linear,
      stroke var(--motion-enter) ease;
  }

  /* Centred inside the ring, which is why the dial is positioned and this is
     absolute: a grid cell would push the label down instead. */
  .middle {
    position: absolute;
    top: 0;
    display: grid;
    place-items: center;
    width: 116px;
    height: 116px;
  }

  .figure {
    color: var(--text-1);
    font-size: var(--text-display);
    font-weight: var(--weight-medium);
    font-variant-numeric: tabular-nums;
    line-height: 1.1;
  }

  .unit,
  .label {
    color: var(--text-3);
    font-size: var(--text-meta);
  }

  .label {
    padding-top: var(--space-2);
    color: var(--text-2);
  }

  .heaviest {
    padding-top: var(--space-1);
  }

  .heading,
  .who {
    display: block;
    color: var(--text-2);
    font-size: var(--text-meta);
  }

  .programs {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    margin: 0;
    padding: var(--space-3) 0 0;
    list-style: none;
  }

  /* Name, bar, figure. The bar takes what is left so the two text columns stay
     put and the eye can run straight down them. */
  .program {
    display: grid;
    grid-template-columns: minmax(0, 9rem) 1fr auto;
    align-items: center;
    gap: var(--space-3);
  }

  .name {
    color: var(--text-1);
    font-size: var(--text-meta);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .meter {
    height: 4px;
    border-radius: var(--radius-pill);
    background: var(--fill-2);
    overflow: hidden;
  }

  .level {
    display: block;
    height: 100%;
    border-radius: var(--radius-pill);
    background: var(--accent-green);
    opacity: 0.55;
    transition: width 1s linear;
  }

  .cost {
    color: var(--text-2);
    font-size: var(--text-meta);
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }

  .cpu {
    padding-left: var(--space-2);
    color: var(--accent-orange);
  }

  .mine {
    display: flex;
    align-items: baseline;
    gap: var(--space-2);
    margin-top: auto;
    padding-top: var(--space-3);
    box-shadow: inset 0 1px 0 var(--hairline);
  }

  .weight {
    color: var(--text-1);
    font-size: var(--text-body);
    font-weight: var(--weight-medium);
    font-variant-numeric: tabular-nums;
  }

  .among {
    flex: 1;
    text-align: right;
    color: var(--text-4);
    font-size: var(--text-meta);
  }

  .quiet {
    margin: 0;
    padding: var(--space-3);
    color: var(--text-3);
    font-size: var(--text-meta);
  }
</style>

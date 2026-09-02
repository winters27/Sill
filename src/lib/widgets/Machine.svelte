<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { pollWhileVisible } from "$lib/visible";

  interface Props {
    /** The chin is a strip, not a board. Same widget, less of it. */
    compact?: boolean;
  }

  let { compact = false }: Props = $props();

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
    sillProcesses: number;
  };

  let reading = $state<Reading | null>(null);
  let failed = $state("");

  /**
   * How often the machine is asked.
   *
   * A second is what every system monitor settles on: faster reads as noise
   * rather than information, because the numbers move more than the thing they
   * describe does. The poll lives and dies with the widget, which for this
   * feature in particular would be embarrassing to get wrong.
   */
  const EVERY = 1_000;

  /** The ring's geometry, in the viewBox's own units. */
  const R = 34;
  const CIRCUMFERENCE = 2 * Math.PI * R;

  /** How many programs are named. Three fits; five did not. */
  const NAMED = 3;

  /**
   * The last minute of processor load, for the trace under the gauges.
   *
   * A number on its own says what is happening; a line says whether it is
   * unusual, which is the question somebody actually opened this to answer.
   * Sixty readings at one a second is a minute, which is long enough to show a
   * spike and short enough that it is still about now.
   *
   * Kept here rather than in Rust deliberately. It exists only while the
   * widget is on screen and goes when it closes, so nothing accumulates a
   * history of the machine behind anybody's back.
   */
  const SPAN = 60;
  let trace = $state<number[]>([]);

  /**
   * The trace as a path, in a 100 by 30 box.
   *
   * Spread across the full width whatever the sample count, so the line spans
   * the box from the first second and grows denser rather than longer. The
   * first version anchored the newest reading to the right edge and let the
   * line reach leftwards as samples arrived, which is defensible and looks
   * exactly like a rendering fault: a short green scribble in the top corner
   * of an otherwise empty box.
   */
  const line = $derived.by(() => {
    if (trace.length < 2) return "";

    const step = 100 / (trace.length - 1);
    const points = trace.map((value, at) => {
      const x = at * step;
      const y = 30 - (Math.min(value, 100) / 100) * 28 - 1;
      return `${x.toFixed(2)},${y.toFixed(2)}`;
    });

    return `M${points.join(" L")}`;
  });

  const memoryPercent = $derived(
    reading && reading.memoryTotal > 0
      ? (reading.memoryUsed / reading.memoryTotal) * 100
      : 0,
  );

  const heaviest = $derived(
    reading?.top.reduce((most, one) => Math.max(most, one.bytes), 0) ?? 0,
  );

  /**
   * Green, amber, red.
   *
   * Not the selection accent, deliberately: that colour means "this is the one
   * you picked" everywhere else in Sill, and a gauge is not a selection.
   */
  function tone(percent: number): string {
    if (percent >= 85) return "var(--accent-red)";
    if (percent >= 60) return "var(--accent-orange)";
    return "var(--accent-green)";
  }

  function gb(bytes: number): string {
    return (bytes / 1_073_741_824).toFixed(1);
  }

  function mb(bytes: number): string {
    return `${Math.round(bytes / 1_048_576)} MB`;
  }

  async function take() {
    try {
      const next = await invoke<Reading>("machine_reading");
      reading = next;
      failed = "";

      // The first reading after opening has no interval behind it and is
      // always zero, so it would draw a dip that never happened.
      if (trace.length > 0 || next.cpu > 0) {
        trace = [...trace, next.cpu].slice(-SPAN);
      }
    } catch (err) {
      failed = `${err}`;
    }
  }

  onMount(() => {
    // Stops while the launcher is hidden. A reading nobody can see is work
    // nobody asked for, and this one ran once a second for as long as Sill
    // was running.
    const stop = pollWhileVisible(take, EVERY);

    return () => {
      stop();
      void invoke("forget_machine_reading");
    };
  });
</script>

{#snippet ring(percent: number, label: string, under: string)}
  <div class="gauge">
    <!--
      The arc and the number are stacked in one grid cell rather than one being
      positioned over the other. Absolute placement needs the two to agree
      about a height in pixels, and they stopped agreeing the moment the tile's
      padding changed, which is how the number ended up sitting off-centre in
      its own ring.
    -->
    <div class="dial">
      <svg viewBox="0 0 80 80" aria-hidden="true">
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

      <!-- Always a percentage. A ring is a proportion, and "16.1 of 31.9 GB"
           inside one is a sentence wearing a circle. The amount goes below. -->
      <span class="figure">{Math.round(percent)}<span class="unit">%</span></span>
    </div>

    <span class="label">{label}</span>
    {#if under}<span class="under">{under}</span>{/if}
  </div>
{/snippet}

{#if compact}
  {#if reading}
    <span class="strip">
      <span class="dot" style:background={tone(reading.cpu)}></span>
      {Math.round(reading.cpu)}%
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
      <!--
        Two columns, because this tile is two columns wide.
        Stacked, everything stretched to seven hundred pixels: bars long enough
        to lose the eye between the name and the number, and a tile too tall
        for the window it lives in. The width was already there.
      -->
      <div class="left">
        <div class="gauges">
          {@render ring(reading.cpu, "Processor", `${reading.count} programs`)}
          {@render ring(
            memoryPercent,
            "Memory",
            `${gb(reading.memoryUsed)} of ${gb(reading.memoryTotal)} GB`,
          )}
        </div>

        {#if line}
          <svg class="trace" viewBox="0 0 100 30" preserveAspectRatio="none" aria-hidden="true">
            <path class="stroke" d={line} stroke={tone(reading.cpu)} />
          </svg>
        {/if}
      </div>

      <div class="right">
        <span class="heading">Heaviest right now</span>

        <ul class="programs">
          {#each reading.top.slice(0, NAMED) as one (one.name + one.bytes)}
            <li class="program">
              <span class="name">{one.name}</span>

              <!-- Against the heaviest rather than against the machine's
                   memory: three programs at four percent of thirty-two
                   gigabytes each is three bars too short to compare, and
                   comparing them is why they are here. -->
              <span class="meter" aria-hidden="true">
                <span
                  class="level"
                  style:width={`${heaviest > 0 ? (one.bytes / heaviest) * 100 : 0}%`}
                ></span>
              </span>

              <span class="cost">{mb(one.bytes)}</span>
            </li>
          {/each}
        </ul>

        <!-- Sill's own weight. The project's claim is that it idles at almost
             nothing, and the honest place to say so is on the same screen as
             everything else, measured the same way. -->
        <p class="mine">
          Sill is using {mb(reading.sill)} across {reading.sillProcesses}
          {reading.sillProcesses === 1 ? "process" : "processes"}
        </p>
      </div>
    {/if}
  </div>
{/if}

<style>
  .readout {
    display: grid;
    /* The gauges need what they need; the list takes the rest. */
    grid-template-columns: auto minmax(0, 1fr);
    gap: var(--space-4);
    align-items: start;
    width: 100%;
    padding: var(--space-3) var(--space-4);
  }

  .left {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .right {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    min-width: 0;
    /* Lines the list up with the tops of the rings rather than with the tile,
       so the two columns read as one row of content. */
    padding-top: 2px;
  }

  .heading {
    color: var(--text-3);
    font-size: var(--text-label);
  }

  .gauges {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: var(--space-3);
  }

  .gauge {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 2px;
  }

  /* One cell, two children, both centred in it. */
  .dial {
    display: grid;
    place-items: center;
    width: 92px;
    height: 92px;
  }

  .dial > :global(*) {
    grid-area: 1 / 1;
  }

  .dial svg {
    width: 92px;
    height: 92px;
    /* Twelve o'clock, rather than three. */
    transform: rotate(-90deg);
  }

  .track,
  .arc {
    fill: none;
    stroke-width: 7;
  }

  .track {
    stroke: var(--fill-2);
  }

  .arc {
    stroke-linecap: round;
    /* Matched to the poll, so the arc glides rather than stepping. */
    transition:
      stroke-dashoffset 1s linear,
      stroke var(--motion-enter) ease;
  }

  .figure {
    color: var(--text-1);
    font-size: var(--text-display);
    font-weight: var(--weight-medium);
    font-variant-numeric: tabular-nums;
    line-height: 1;
  }

  .unit {
    padding-left: 1px;
    color: var(--text-3);
    font-size: var(--text-body);
    font-weight: var(--weight-body);
  }

  .label {
    padding-top: var(--space-2);
    color: var(--text-2);
    font-size: var(--text-meta);
  }

  .under {
    color: var(--text-4);
    font-size: var(--text-label);
    font-variant-numeric: tabular-nums;
  }

  /*
   * The last minute, drawn edge to edge.
   *
   * No axes and no grid: it is not a chart anybody reads values off, it is the
   * shape of the last minute, and every line drawn around it would be more ink
   * than the thing it frames.
   */
  .trace {
    width: 100%;
    height: 26px;
    /* Non-uniform scaling means a stroke width in user units would come out
       stretched, so it is given in pixels instead. */
    vector-effect: non-scaling-stroke;
  }

  .stroke {
    fill: none;
    stroke-width: 1.5px;
    stroke-linejoin: round;
    stroke-linecap: round;
    vector-effect: non-scaling-stroke;
    opacity: 0.85;
    transition: stroke var(--motion-enter) ease;
  }

  .programs {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    margin: 0;
    padding: 0;
    list-style: none;
  }

  /* Name, bar, figure. The two text columns are fixed so the eye runs straight
     down them and the bar takes whatever is left. */
  .program {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 5rem 4.5rem;
    align-items: center;
    gap: var(--space-3);
  }

  .name {
    color: var(--text-2);
    font-size: var(--text-label);
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
    opacity: 0.5;
    transition: width 1s linear;
  }

  .cost {
    color: var(--text-3);
    font-size: var(--text-label);
    font-variant-numeric: tabular-nums;
    text-align: right;
  }

  .mine {
    margin: 0;
    margin-top: auto;
    padding-top: var(--space-2);
    color: var(--text-4);
    font-size: var(--text-label);
  }

  .quiet {
    margin: 0;
    color: var(--text-3);
    font-size: var(--text-meta);
  }

  /* The chin: a dot, the load, and what Sill costs. */
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
</style>

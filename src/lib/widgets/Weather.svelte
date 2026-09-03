<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { pollWhileVisible } from "$lib/visible";

  interface Props {
    compact?: boolean;
  }

  let { compact = false }: Props = $props();

  type Weather = {
    place: string;
    temperature: number;
    feelsLike: number;
    high: number;
    low: number;
    code: number;
    isDay: boolean;
    unit: string;
  };

  let sky = $state<Weather | null>(null);
  let failed = $state("");

  /**
   * How often the sky is asked about.
   *
   * Ten minutes, because that is roughly how often the service itself updates
   * and asking faster is asking somebody else's server for the same answer.
   * The timer lives and dies with the widget.
   */
  const EVERY = 10 * 60 * 1_000;

  /**
   * WMO weather codes, which is what the service speaks.
   *
   * Grouped rather than enumerated: the standard separates light, moderate and
   * heavy drizzle, and nobody glancing at a launcher needs three words for
   * drizzle. Anything unrecognised falls through to the cloud, which is the
   * least wrong thing to draw for weather nobody mapped.
   */
  function describe(code: number, day: boolean): { glyph: string; word: string } {
    if (code === 0) return { glyph: day ? "☀" : "☾", word: day ? "Clear" : "Clear night" };
    if (code <= 2) return { glyph: day ? "⛅" : "☁", word: "Partly cloudy" };
    if (code === 3) return { glyph: "☁", word: "Overcast" };
    if (code <= 48) return { glyph: "≡", word: "Fog" };
    if (code <= 57) return { glyph: "☂", word: "Drizzle" };
    if (code <= 67) return { glyph: "☂", word: "Rain" };
    if (code <= 77) return { glyph: "❄", word: "Snow" };
    if (code <= 82) return { glyph: "☂", word: "Showers" };
    if (code <= 86) return { glyph: "❄", word: "Snow showers" };
    if (code <= 99) return { glyph: "⚡", word: "Thunderstorms" };
    return { glyph: "☁", word: "Cloudy" };
  }

  const shown = $derived(sky ? describe(sky.code, sky.isDay) : null);

  function degrees(value: number): string {
    return `${Math.round(value)}°`;
  }

  async function take() {
    try {
      sky = await invoke<Weather>("weather_now");
      failed = "";
    } catch (err) {
      failed = `${err}`;
    }
  }

  onMount(() => {
    // Stops while the launcher is hidden. Ten minutes apart is not much, and
    // it is still a network call made on behalf of a window nobody can see.
    return pollWhileVisible(take, EVERY);
  });
</script>

{#if compact}
  {#if sky && shown}
    <span class="strip">
      <span class="glyph">{shown.glyph}</span>
      {degrees(sky.temperature)}
    </span>
  {/if}
{:else if failed}
  <div class="face">
    <p class="quiet">{failed}</p>
  </div>
{:else if !sky || !shown}
  <div class="face">
    <p class="quiet">Looking outside…</p>
  </div>
{:else}
  <div class="face">
    <!-- The glyph sits beside the temperature rather than pushed to the far
         corner, where it collided with the tile's pin. Two things fighting for
         the same twenty pixels, and the one that can be moved is the one that
         is only decoration. -->
    <div class="head">
      <span class="temperature">{degrees(sky.temperature)}</span>
      <span class="glyph big">{shown.glyph}</span>
    </div>

    <span class="word">{shown.word}</span>

    <div class="foot">
      <span class="place">{sky.place}</span>
      <span class="range">
        {degrees(sky.high)} / {degrees(sky.low)}
      </span>
    </div>
  </div>
{/if}

<style>
  .face {
    display: flex;
    flex-direction: column;
    height: 100%;
    padding: var(--space-4);
  }

  .head {
    display: flex;
    align-items: baseline;
    gap: var(--space-2);
    /* Room for the tile's pin, which lives in the top right corner of every
       tile and does not move for anybody. */
    padding-right: var(--icon-tile);
  }

  .temperature {
    color: var(--text-1);
    font-size: var(--text-hero);
    font-weight: var(--weight-medium);
    font-variant-numeric: tabular-nums;
    letter-spacing: -0.02em;
    line-height: 1;
  }

  /* Large, and deliberately not coloured. A yellow sun and a blue cloud would
     be the only two saturated things on the board and would pull the eye off
     the number, which is what somebody came to read. */
  .glyph.big {
    color: var(--text-2);
    font-size: var(--text-title);
    line-height: 1.2;
  }

  .word {
    padding-top: var(--space-1);
    color: var(--text-2);
    font-size: var(--text-body);
  }

  .foot {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: var(--space-2);
    margin-top: auto;
    padding-top: var(--space-3);
  }

  .place {
    color: var(--text-3);
    font-size: var(--text-meta);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .range {
    flex: none;
    color: var(--text-4);
    font-size: var(--text-meta);
    font-variant-numeric: tabular-nums;
  }

  .quiet {
    margin: 0;
    color: var(--text-3);
    font-size: var(--text-meta);
    line-height: 1.5;
  }

  .strip {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    color: var(--text-1);
    font-size: var(--text-meta);
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }

  .strip .glyph {
    color: var(--text-3);
  }
</style>

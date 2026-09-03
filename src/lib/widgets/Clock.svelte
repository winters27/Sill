<script lang="ts">
  import { onMount } from "svelte";

  interface Props {
    /** The chin is a strip, not a board. Same widget, less of it. */
    compact?: boolean;
    /** Counting seconds costs a redraw a second for as long as this is up. */
    seconds?: boolean;
  }

  let { compact = false, seconds = false }: Props = $props();

  let now = $state(new Date());

  /**
   * Ticks on the minute, or on the second when asked.
   *
   * Not `setInterval(1000)` for a clock showing minutes. That is fifty-nine
   * wakeups an hour spent redrawing the same two digits, on a launcher whose
   * whole argument is that it does nothing while nothing is happening. The
   * timeout is set to land just past the next boundary and then re-set, so
   * there is exactly one wakeup per visible change.
   */
  onMount(() => {
    let timer: ReturnType<typeof setTimeout>;

    const tick = () => {
      now = new Date();

      const period = seconds ? 1_000 : 60_000;
      const since = seconds ? now.getMilliseconds() : now.getSeconds() * 1_000 + now.getMilliseconds();

      // The extra few milliseconds keep it from firing a hair early and
      // drawing the same time twice.
      timer = setTimeout(tick, period - since + 20);
    };

    tick();
    return () => clearTimeout(timer);
  });

  const time = $derived(
    now.toLocaleTimeString([], {
      hour: "numeric",
      minute: "2-digit",
      ...(seconds ? { second: "2-digit" } : {}),
    }),
  );

  const day = $derived(
    now.toLocaleDateString([], { weekday: "long", month: "long", day: "numeric" }),
  );

  /** How far through the day it is, for the arc under the time. */
  const through = $derived(
    ((now.getHours() * 60 + now.getMinutes()) / (24 * 60)) * 100,
  );
</script>

{#if compact}
  <span class="strip">{time}</span>
{:else}
  <div class="face">
    <span class="time">{time}</span>
    <span class="day">{day}</span>

    <!-- How far through the day, which is the one thing a clock cannot say by
         reading it: half past nine means something different in June. -->
    <span class="through" aria-hidden="true">
      <span class="passed" style:width={`${through}%`}></span>
    </span>
  </div>
{/if}

<style>
  .face {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: var(--space-1);
    height: 100%;
    padding: var(--space-4);
  }

  /*
   * The hour, and it is meant to dominate.
   *
   * Tabular figures so the digits do not shuffle sideways as they change,
   * which on a clock is the difference between a widget and a fidget.
   */
  .time {
    color: var(--text-1);
    font-size: var(--text-hero);
    font-weight: var(--weight-medium);
    font-variant-numeric: tabular-nums;
    letter-spacing: -0.02em;
    line-height: 1;
  }

  .day {
    color: var(--text-3);
    font-size: var(--text-meta);
  }

  .through {
    display: block;
    width: 100%;
    height: 3px;
    margin-top: auto;
    border-radius: var(--radius-pill);
    background: var(--fill-2);
    overflow: hidden;
  }

  .passed {
    display: block;
    height: 100%;
    border-radius: var(--radius-pill);
    background: var(--accent);
    opacity: var(--opacity-muted);
  }

  .strip {
    color: var(--text-1);
    font-size: var(--text-meta);
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
</style>

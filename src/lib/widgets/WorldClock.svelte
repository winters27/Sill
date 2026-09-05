<script lang="ts">
  /**
   * The time in other cities.
   *
   * Rust resolves each chosen city to the zone name the browser's own clock
   * understands, once, when the list changes. The ticking is then the same
   * as the clock's: the machine's own time, formatted for that zone, with
   * nothing asked of Rust on any tick. That is what keeps a pinned world
   * clock free while the launcher is hidden: a timer that reached Rust once
   * a minute would run on for a window nobody could see.
   */
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { orElse } from "$lib/status";

  interface Props {
    /** The chin is a strip, not a board. Same widget, less of it. */
    compact?: boolean;
    /** The cities chosen in settings, by name. */
    clocks?: string[];
  }

  let { compact = false, clocks = [] }: Props = $props();

  interface Shown {
    city: string;
    iana: string | null;
  }

  let cities = $state<Shown[]>([]);
  let now = $state(new Date());

  /**
   * Resolved again whenever the list changes, which is a settings change and
   * nothing else. An empty list asks nothing.
   */
  $effect(() => {
    const wanted = clocks.join("\n");
    if (!wanted) {
      cities = [];
      return;
    }

    void invoke<Shown[]>("world_clocks")
      .catch(orElse("launcher", "which cities the world clock shows", [], "widgets"))
      .then((list) => {
        cities = Array.isArray(list) ? list : [];
      });
  });

  // One wakeup per minute boundary, exactly as the clock does it.
  onMount(() => {
    let timer: ReturnType<typeof setTimeout>;

    const tick = () => {
      now = new Date();
      const since = now.getSeconds() * 1_000 + now.getMilliseconds();
      timer = setTimeout(tick, 60_000 - since + 20);
    };

    tick();
    return () => clearTimeout(timer);
  });

  /** The clock in a zone, or a dash when the zone is not one this machine knows. */
  function clock(iana: string | null): string {
    if (!iana) return "–";

    try {
      return now.toLocaleTimeString([], { hour: "numeric", minute: "2-digit", timeZone: iana });
    } catch (reason) {
      // A zone name the browser does not know. Rust vouched for it, so this
      // is a browser older than the table, and a dash says so honestly.
      return "–";
    }
  }
</script>

{#if compact}
  <span class="strip">
    {#each cities as one (one.city)}
      <span class="pair"><span class="city">{one.city}</span> {clock(one.iana)}</span>
    {/each}
    {#if !cities.length}
      <span class="city">No cities</span>
    {/if}
  </span>
{:else}
  <div class="face">
    {#if cities.length}
      <ul class="cities">
        {#each cities as one (one.city)}
          <li>
            <span class="time">{clock(one.iana)}</span>
            <span class="name">{one.city}</span>
          </li>
        {/each}
      </ul>
    {:else}
      <!-- The weather widget's own quiet line, for a tile with nothing chosen
           yet. Not `Instead`, which is a view-sized recipe and this is a
           tile. -->
      <p class="quiet">Choose cities in Settings, under Widgets.</p>
    {/if}
  </div>
{/if}

<style>
  .face {
    display: flex;
    flex-direction: column;
    height: 100%;
    padding: var(--space-4);
  }

  .cities {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    margin: 0;
    padding: 0;
    list-style: none;
  }

  li {
    display: flex;
    align-items: baseline;
    gap: var(--space-2);
  }

  .time {
    color: var(--text-1);
    font-size: var(--text-title);
    font-weight: var(--weight-medium);
    font-variant-numeric: tabular-nums;
    line-height: 1.2;
  }

  .name,
  .quiet {
    color: var(--text-3);
    font-size: var(--text-meta);
  }

  .quiet {
    margin: 0;
  }

  .strip {
    display: inline-flex;
    gap: var(--space-2);
    color: var(--text-1);
    font-size: var(--text-meta);
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }

  .city {
    color: var(--text-3);
  }
</style>

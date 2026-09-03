<script lang="ts">
  import Clock from "./Clock.svelte";
  import Weather from "./Weather.svelte";
  import Machine from "./Machine.svelte";
  import { widget } from "./registry";
  import type { Preferences } from "$lib/settings";

  interface Props {
    prefs: Preferences | null;
  }

  let { prefs }: Props = $props();

  /**
   * What is pinned, in the order it was pinned, ignoring anything unknown.
   *
   * Filtered rather than trusted: preferences outlive the build that wrote
   * them, so an id from a newer Sill, or one whose widget was removed, must
   * draw nothing rather than crash the chin the launcher is standing on.
   */
  const shown = $derived(
    (prefs?.widgets.pinned ?? []).filter((id) => widget(id) !== undefined),
  );
</script>

{#if shown.length}
  <div class="pinned">
    {#each shown as id (id)}
      <span class="one">
        {#if id === "clock"}
          <Clock compact seconds={prefs?.widgets.seconds ?? false} />
        {:else if id === "weather"}
          <Weather compact />
        {:else if id === "machine"}
          <Machine compact />
        {/if}
      </span>
    {/each}
  </div>
{/if}

<style>
  /*
   * The chin is a row of hints, so this has to read as one of them rather than
   * as a panel that landed in the footer. No fill, no border: just text at the
   * weight everything else down here is, separated by hairlines.
   */
  .pinned {
    display: flex;
    align-items: center;
    min-width: 0;
    overflow: hidden;
  }

  .one {
    display: inline-flex;
    align-items: center;
    padding: 0 var(--space-3);
    white-space: nowrap;
  }

  /* Between, never at the ends, so the strip does not grow a rule against the
     status text on one side or the Escape hint on the other. */
  .one + .one {
    box-shadow: var(--ring-left);
  }
</style>

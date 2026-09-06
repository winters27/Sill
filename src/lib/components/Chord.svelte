<!--
  A chord, drawn as one keycap per key.

  The one way a chord is drawn. There were four: the raw accelerator with its
  plus signs, the same with the pluses swapped for spaces, one keycap around
  the whole chord, and per-key caps built from a Raycast shortcut object. The
  settings panel showed three of them on one screen.

  Draws nothing for an empty chord, so a caller never gets one empty cap.
-->
<script lang="ts">
  import { keysOf } from "$lib/keys";

  interface Props {
    chord: string;
    /** Quieter, for a chord that is shown rather than set: the keys held so far. */
    dim?: boolean;
  }

  let { chord, dim = false }: Props = $props();

  const keys = $derived(keysOf(chord));
</script>

{#if keys.length > 0}
  <span class="chord" class:dim>
    {#each keys as key, at (at)}
      <kbd class="sill-key">{key}</kbd>
    {/each}
  </span>
{/if}

<style>
  .chord {
    display: inline-flex;
    align-items: center;
    gap: var(--space-half);
  }

  .dim {
    opacity: var(--opacity-muted);
  }
</style>

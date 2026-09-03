<script lang="ts">
  import type { Snippet } from "svelte";

  interface Props {
    label: string;
    description?: string;
    /** Rows lay themselves out; use for controls that are not settings rows. */
    bare?: boolean;
    children: Snippet;
  }

  let { label, description, bare = false, children }: Props = $props();
</script>

<!--
  A section is the card. A row is not.

  Every setting in its own bevelled card made a panel read as a stack of
  competing floating boxes. One flat slab per group, with hairlines between the
  rows inside it, says "these belong together" without any of that.
-->
<section>
  <div class="head">
    <h3>{label}</h3>
    {#if description}<p>{description}</p>{/if}
  </div>

  {#if bare}
    <div class="bare">{@render children()}</div>
  {:else}
    <div class="sill-card">{@render children()}</div>
  {/if}
</section>

<style>
  section {
    margin-bottom: var(--space-6);
  }

  .head {
    padding: 0 var(--space-half) var(--space-2);
  }

  /*
   * A label, not a headline.
   *
   * This was white at 12px with 0.14em of tracking, which made it louder than
   * the panel title above it. That is backwards: the title names what is on
   * screen and the section label only says which part of it this is.
   */
  h3 {
    margin: 0;
    font-size: var(--text-label);
    font-weight: var(--weight-strong);
    letter-spacing: var(--track-label);
    text-transform: uppercase;
    color: var(--text-3);
  }

  p {
    margin: var(--space-1) 0 0;
    /* A line of prose stops being readable somewhere past 80 characters, and
       a wide settings pane will happily run to 160. */
    max-width: 82ch;
    font-size: var(--text-meta);
    line-height: 1.55;
    color: var(--text-2);
  }

  .bare {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }
</style>

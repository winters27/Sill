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
    margin-bottom: 26px;
  }

  .head {
    padding: 0 2px 10px;
  }

  h3 {
    margin: 0;
    font-size: 12px;
    font-weight: 600;
    letter-spacing: 0.14em;
    text-transform: uppercase;
    color: var(--core-foreground);
  }

  p {
    margin: 5px 0 0;
    /* A line of prose stops being readable somewhere past 80 characters, and
       a wide settings pane will happily run to 160. */
    max-width: 82ch;
    font-size: 12px;
    line-height: 1.55;
    color: var(--text-muted);
  }

  .bare {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
</style>

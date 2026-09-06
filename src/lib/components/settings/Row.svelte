<script lang="ts">
  import type { Snippet } from "svelte";

  interface Props {
    title: string;
    description?: string;
    /** Greyed and inert, for a setting its parent switch has turned off. */
    disabled?: boolean;
    /** The control on the right. */
    control?: Snippet;
    /** A wider control that needs the full row width, drawn underneath. */
    children?: Snippet;
  }

  let { title, description, disabled = false, control, children }: Props = $props();
</script>

<div class="sill-setting" class:disabled>
  <div class="line">
    <div class="label">
      <span class="name">{title}</span>
      {#if description}<span class="hint">{description}</span>{/if}
    </div>
    {#if control}
      <div class="control">{@render control()}</div>
    {/if}
  </div>

  {#if children}
    <div class="wide">{@render children()}</div>
  {/if}
</div>

<style>
  .disabled {
    opacity: 0.45;
    pointer-events: none;
  }

  .line {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: var(--space-6);
  }

  .label {
    min-width: 0;
    flex: 1;
  }

  .name {
    display: block;
    font-size: var(--text-body);
    font-weight: var(--weight-medium);
    color: var(--text-1);
  }

  .hint {
    display: block;
    margin-top: var(--space-1);
    /* Shorter than a section's, because a row's control sits at the end of
       the same line and prose running under it reads as a collision. */
    max-width: 62ch;
    font-size: var(--text-meta);
    line-height: 1.55;
    color: var(--text-2);
  }

  .control {
    flex: none;
    /* Half a line, so a control lines up with the title rather than the
       middle of a two-line label. */
    padding-top: var(--space-hair);
  }

  .wide {
    margin-top: var(--space-4);
  }
</style>

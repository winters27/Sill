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
    gap: 20px;
  }

  .label {
    min-width: 0;
    flex: 1;
  }

  .name {
    display: block;
    font-size: 13px;
    font-weight: 500;
    color: var(--core-foreground);
  }

  .hint {
    display: block;
    margin-top: 3px;
    /* Shorter than a section's, because a row's control sits at the end of
       the same line and prose running under it reads as a collision. */
    max-width: 68ch;
    font-size: 12px;
    line-height: 1.55;
    color: var(--text-muted);
  }

  .control {
    flex: none;
    /* Half a line, so a control lines up with the title rather than the
       middle of a two-line label. */
    padding-top: 1px;
  }

  .wide {
    margin-top: 12px;
  }
</style>

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

  // One id per row, so the control inside is announced with its name and the
  // sentence under it, including an error written there about the setting.
  const id = $props.id();
</script>

<!--
  A group named by the row, so a screen reader entering the control hears
  which setting it is and what the line under it says.

  Disabled is `inert` on the controls, not only greyed and unclickable: a
  greyed row whose switch could still be reached with Tab and flipped with
  Space was a setting its parent had turned off and the keyboard had not.
  The label stays readable, so the row still says what it is.
-->
<div
  class="sill-setting"
  class:disabled
  role="group"
  aria-labelledby="{id}-name"
  aria-describedby={description ? `${id}-hint` : undefined}
  aria-disabled={disabled || undefined}
>
  <div class="line">
    <div class="label">
      <span class="name" id="{id}-name">{title}</span>
      {#if description}<span class="hint" id="{id}-hint">{description}</span>{/if}
    </div>
    {#if control}
      <div class="control" inert={disabled}>{@render control()}</div>
    {/if}
  </div>

  {#if children}
    <div class="wide" inert={disabled}>{@render children()}</div>
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

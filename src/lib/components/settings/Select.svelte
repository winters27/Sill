<script lang="ts">
  /**
   * The one picker the settings window uses.
   *
   * Windows draws the open list itself, and it starts white however the page
   * is painted. Every panel that wrote its own `<select>` had to remember to
   * colour the options as well, and the ones that forgot rendered white text
   * on a white list, readable only on the highlighted row. Deciding it once
   * here is the point of this file.
   */
  interface Choice {
    /** What is stored. */
    value: string;
    /** What is read. */
    label: string;
  }

  interface Props {
    value: string;
    options: Choice[];
    onchange: (value: string) => void;
    /** Fills the row rather than sizing to its longest label. */
    full?: boolean;
    /**
     * Holds one width whatever is chosen.
     *
     * A picker that sizes to its current label moves the row every time the
     * choice changes, which reads as the layout being unstable.
     */
    steady?: boolean;
    /**
     * A header control rather than a form field.
     *
     * No ground and no outline, because it sits above a panel and reads as a
     * label you can change rather than something waiting to be filled in.
     */
    quiet?: boolean;
    disabled?: boolean;
    /** Needed whenever the row's own title is not beside it. */
    ariaLabel?: string;
  }

  let {
    value,
    options,
    onchange,
    full = false,
    steady = false,
    quiet = false,
    disabled = false,
    ariaLabel,
  }: Props = $props();
</script>

<select
  {value}
  {disabled}
  class:full
  class:steady
  class:quiet
  aria-label={ariaLabel}
  onchange={(event) => onchange(event.currentTarget.value)}
>
  {#each options as option (option.value)}
    <option value={option.value}>{option.label}</option>
  {/each}
</select>

<style>
  select {
    padding: var(--space-1) var(--space-2);
    border: 0;
    border-radius: var(--radius-sm);
    background: var(--fill-1);
    box-shadow: var(--ring);
    color: var(--text-1);
    font: inherit;
    font-size: var(--text-meta);
    outline: none;
    cursor: pointer;
    transition: box-shadow var(--motion-state) var(--ease);
  }

  select:focus {
    box-shadow: var(--ring-strong);
  }

  select:disabled {
    opacity: var(--opacity-disabled);
    cursor: default;
  }

  .full {
    width: 100%;
  }

  .steady {
    min-width: 210px;
  }

  .quiet {
    padding: var(--space-half) var(--space-1);
    background: transparent;
    box-shadow: none;
    color: var(--text-2);
    transition: color var(--motion-state) var(--ease);
  }

  .quiet:hover,
  .quiet:focus {
    box-shadow: none;
    color: var(--text-1);
  }

  /* The list Windows opens is its own window and starts white. Without this
     the labels are light text on it and only the highlighted row can be read. */
  select option {
    background: var(--core-secondary-background);
    color: var(--text-1);
  }
</style>

<script lang="ts">
  /**
   * The one text field the settings window uses.
   *
   * Same reason as the picker beside it: a field with no rule of its own is
   * drawn by Windows, which paints it white whatever the panel behind it
   * looks like.
   */
  interface Props {
    value: string;
    oninput?: (value: string) => void;
    /**
     * Called when the field is left or Enter is pressed, rather than per key.
     *
     * For a field whose every keystroke should not be written: an extension's
     * API key saved on `input` is one write per character and a half-typed
     * token stored between them. A debounce would be the other answer, and a
     * debounce has to be flushed on unmount, which is how a setting gets lost
     * in a window that can be closed at any moment.
     */
    onchange?: (value: string) => void;
    placeholder?: string;
    /** Fills the row rather than taking its natural width. */
    full?: boolean;
    disabled?: boolean;
    /** Hides what is typed, for anything that should not be read over a shoulder. */
    secret?: boolean;
    /** Addresses and keys are read character by character, so they get the mono face. */
    mono?: boolean;
    ariaLabel?: string;
  }

  let {
    value,
    oninput,
    onchange,
    placeholder,
    full = false,
    disabled = false,
    secret = false,
    mono = false,
    ariaLabel,
  }: Props = $props();
</script>

<input
  type={secret ? "password" : "text"}
  {value}
  {disabled}
  {placeholder}
  class:full
  class:mono
  aria-label={ariaLabel}
  spellcheck="false"
  autocomplete="off"
  oninput={(event) => oninput?.(event.currentTarget.value)}
  onchange={(event) => onchange?.(event.currentTarget.value)}
/>

<style>
  input {
    padding: var(--space-1) var(--space-2);
    border: 0;
    border-radius: var(--radius-sm);
    background: var(--fill-1);
    box-shadow: var(--ring);
    color: var(--text-1);
    font: inherit;
    font-size: var(--text-meta);
    outline: none;
    user-select: text;
    transition: box-shadow var(--motion-state) var(--ease);
  }

  /* Focus is one of the four things the accent is for, and the sidebar's
     search field already says so; every other field disagreed with it. */
  input:focus {
    box-shadow: var(--ring-accent-faint);
  }

  input:disabled {
    opacity: var(--opacity-disabled);
  }

  input::placeholder {
    color: var(--text-3);
  }

  .full {
    width: 100%;
  }

  .mono {
    font-family: var(--font-mono);
  }
</style>

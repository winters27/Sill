<script lang="ts">
  interface Props {
    label: string;
    onclick: () => void;
    /** Destructive actions get the one colour that says so. */
    tone?: "normal" | "danger";
    busy?: boolean;
    /**
     * The id of the sentence that says what pressing this will do.
     *
     * For a button whose own label cannot carry the consequence: "Remove"
     * beside a question about emptying two folders that cannot be undone. A
     * reader hears the label and then the sentence, which is the whole of what
     * a sighted reader gets from the two being next to each other.
     */
    describedBy?: string;
  }

  let {
    label,
    onclick,
    tone = "normal",
    busy = false,
    describedBy = undefined,
  }: Props = $props();
</script>

<!--
  `aria-disabled` rather than `disabled` while busy.

  A disabled element cannot hold focus, so disabling the button somebody just
  pressed moved focus to the document body and left a keyboard user tabbing in
  from the top of the page to reach the next row. This stays focused, announces
  as unavailable, and refuses the press in the handler.
-->
<button
  class:danger={tone === "danger"}
  class:busy
  aria-disabled={busy}
  aria-describedby={describedBy}
  onclick={() => {
    if (busy) return;
    onclick();
  }}
>
  {busy ? "Working…" : label}
</button>

<style>
  button {
    padding: var(--space-1) var(--space-3);
    border: 0;
    border-radius: var(--radius-sm);
    background: var(--fill-2);
    box-shadow: var(--bevel-tile);
    color: var(--text-1);
    font: inherit;
    font-size: var(--text-meta);
    font-weight: var(--weight-medium);
    white-space: nowrap;
    cursor: pointer;
    transition:
      background-color var(--motion-state) var(--ease),
      color var(--motion-state) var(--ease);
  }

  button:hover:not(:disabled) {
    background: var(--hairline-strong);
  }

  button:disabled,
  .busy {
    opacity: var(--opacity-disabled);
    cursor: default;
  }

  .busy:hover {
    background: var(--fill-2);
  }

  .danger {
    color: var(--danger);
  }

  .danger:hover:not(:disabled) {
    background: var(--danger-fill);
  }
</style>

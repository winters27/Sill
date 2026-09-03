<script lang="ts">
  interface Props {
    label: string;
    onclick: () => void;
    /** Destructive actions get the one colour that says so. */
    tone?: "normal" | "danger";
    busy?: boolean;
  }

  let { label, onclick, tone = "normal", busy = false }: Props = $props();
</script>

<button class:danger={tone === "danger"} disabled={busy} {onclick}>
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

  button:disabled {
    opacity: var(--opacity-disabled);
    cursor: default;
  }

  .danger {
    color: var(--danger);
  }

  .danger:hover:not(:disabled) {
    background: var(--danger-fill);
  }
</style>

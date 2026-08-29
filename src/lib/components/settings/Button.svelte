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
    padding: 6px 14px;
    border: 0;
    border-radius: var(--radius-sm);
    background: rgba(var(--accent-rgb), 0.1);
    box-shadow: var(--bevel-tile);
    color: var(--core-foreground);
    font: inherit;
    font-size: 12.5px;
    font-weight: 500;
    white-space: nowrap;
    cursor: pointer;
    transition: background-color 0.15s var(--ease), color 0.15s var(--ease);
  }

  button:hover:not(:disabled) {
    background: rgba(var(--accent-rgb), 0.2);
  }

  button:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .danger {
    color: var(--accent-red);
  }

  .danger:hover:not(:disabled) {
    background: rgba(var(--accent-red-rgb), 0.14);
  }
</style>

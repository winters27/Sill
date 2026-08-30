<script lang="ts">
  interface Props {
    value: number;
    min: number;
    max: number;
    step?: number;
    /** Drawn beside the track, e.g. "9 rows" or "78%". */
    format?: (value: number) => string;
    onchange: (value: number) => void;
    label: string;
  }

  let { value = $bindable(), min, max, step = 1, format, onchange, label }: Props = $props();

  const shown = $derived(format ? format(value) : String(value));
</script>

<div class="slider">
  <input
    type="range"
    aria-label={label}
    {min}
    {max}
    {step}
    bind:value
    oninput={() => onchange(value)}
  />
  <span class="value">{shown}</span>
</div>

<style>
  .slider {
    display: flex;
    align-items: center;
    gap: var(--space-3);
  }

  .value {
    min-width: 62px;
    font-family: var(--font-mono);
    font-size: var(--text-meta);
    color: var(--text-2);
    text-align: right;
    /* Fixed width, so the track does not shift as the number changes. */
    font-variant-numeric: tabular-nums;
  }

  input[type="range"] {
    width: 168px;
    height: 4px;
    margin: 0;
    appearance: none;
    border-radius: var(--radius-pill);
    background: var(--fill-3);
    box-shadow: inset 0 1px 2px rgba(0, 0, 0, 0.3);
    outline: none;
    cursor: pointer;
  }

  input[type="range"]::-webkit-slider-thumb {
    appearance: none;
    width: 14px;
    height: 14px;
    border-radius: 50%;
    background: var(--core-accent);
    box-shadow:
      0 1px 3px rgba(0, 0, 0, 0.4),
      inset 0 1px 0 rgba(255, 255, 255, 0.25);
    transition: transform 0.15s var(--ease);
  }

  input[type="range"]:hover::-webkit-slider-thumb {
    transform: scale(1.12);
  }

  input[type="range"]:focus-visible {
    box-shadow:
      inset 0 1px 2px rgba(0, 0, 0, 0.3),
      0 0 0 2px var(--hairline-strong);
  }
</style>

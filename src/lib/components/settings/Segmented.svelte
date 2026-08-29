<script lang="ts">
  interface Props {
    value: string;
    options: { value: string; label: string }[];
    onchange: (value: string) => void;
  }

  let { value = $bindable(), options, onchange }: Props = $props();

  let track = $state<HTMLDivElement | null>(null);
  let buttons: Record<string, HTMLButtonElement | null> = {};
  let thumb = $state({ left: 0, width: 0, ready: false, animate: false });
  let measured = false;

  /**
   * The thumb is measured off the active segment rather than assuming equal
   * widths, so labels of different lengths still line up.
   *
   * The first placement is instant. Animating it would slide the thumb in from
   * the corner every time the panel mounts.
   */
  function measure() {
    const el = buttons[value];
    if (!el) return;
    const animate = measured;
    measured = true;
    thumb = { left: el.offsetLeft, width: el.offsetWidth, ready: true, animate };
  }

  $effect(() => {
    value;
    options;
    measure();

    if (!track) return;
    const observer = new ResizeObserver(measure);
    observer.observe(track);
    return () => observer.disconnect();
  });

  function pick(next: string) {
    value = next;
    onchange(next);
  }
</script>

<div class="track" bind:this={track} role="radiogroup">
  <div
    class="thumb"
    class:ready={thumb.ready}
    class:animate={thumb.animate}
    style:left="{thumb.left}px"
    style:width="{thumb.width}px"
    aria-hidden="true"
  ></div>

  {#each options as option (option.value)}
    <button
      type="button"
      role="radio"
      aria-checked={value === option.value}
      class:active={value === option.value}
      bind:this={buttons[option.value]}
      onclick={() => pick(option.value)}
    >
      {option.label}
    </button>
  {/each}
</div>

<style>
  /* A recessed track with one raised thumb sliding between the segments. Three
     detached buttons would read as three actions rather than one choice. */
  .track {
    position: relative;
    display: inline-flex;
    gap: 2px;
    padding: 2px;
    border-radius: 8px;
    background: rgba(var(--accent-rgb), 0.06);
    box-shadow:
      inset 0 1px 2px rgba(0, 0, 0, 0.32),
      inset 0 0 0 1px rgba(var(--accent-rgb), 0.12);
  }

  .thumb {
    position: absolute;
    top: 2px;
    bottom: 2px;
    border-radius: 6px;
    background: rgba(var(--accent-rgb), 0.15);
    box-shadow:
      0 1px 1px rgba(0, 0, 0, 0.22),
      inset 0 0 0 1px rgba(255, 255, 255, 0.06);
    opacity: 0;
    pointer-events: none;
    transition: opacity 0.14s linear;
  }

  .thumb.ready {
    opacity: 1;
  }

  .thumb.animate {
    transition:
      left 0.28s var(--ease),
      width 0.28s var(--ease),
      opacity 0.14s linear;
  }

  button {
    position: relative;
    z-index: 1;
    padding: 5px 13px;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: var(--text-muted);
    font: inherit;
    font-size: 13px;
    font-weight: 500;
    cursor: pointer;
    transition: color 0.15s var(--ease);
  }

  button:hover {
    color: var(--core-foreground);
  }

  button.active {
    color: var(--core-foreground);
  }

  @media (prefers-reduced-motion: reduce) {
    .thumb.animate {
      transition: opacity 0.14s linear;
    }
  }
</style>

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
    gap: var(--space-half);
    padding: var(--space-half);
    border-radius: var(--radius-md);
    background: var(--fill-1);
    box-shadow: var(--well), var(--ring-fill-soft);
  }

  /* The thumb marks which option is chosen, so it is one of the places the
     accent is allowed. The track around it stays neutral. */
  .thumb {
    position: absolute;
    top: 2px;
    bottom: 2px;
    border-radius: var(--radius-md);
    background: var(--accent-fill);
    box-shadow: var(--elevation-thumb), var(--ring);
    opacity: 0;
    pointer-events: none;
    transition: opacity var(--motion-state) linear;
  }

  .thumb.ready {
    opacity: 1;
  }

  .thumb.animate {
    transition:
      left var(--motion-travel) var(--ease),
      width var(--motion-travel) var(--ease),
      opacity var(--motion-state) linear;
  }

  button {
    position: relative;
    z-index: var(--z-raised);
    padding: var(--space-1) var(--space-3);
    border: 0;
    border-radius: var(--radius-md);
    background: transparent;
    color: var(--text-2);
    font: inherit;
    font-size: var(--text-body);
    font-weight: var(--weight-medium);
    cursor: pointer;
    transition: color var(--motion-state) var(--ease);
  }

  button:hover {
    color: var(--text-1);
  }

  button.active {
    color: var(--text-1);
  }

  @media (prefers-reduced-motion: reduce) {
    .thumb.animate {
      transition: opacity var(--motion-state) linear;
    }
  }
</style>

<script lang="ts">
  /**
   * What the model thought before it answered.
   *
   * Open while it is thinking, because that is the only thing on screen
   * saying anything is happening, and folded to one line once the answer
   * starts: the working is worth having and not worth reading every time.
   * Pressing the line reopens it, and that choice then holds.
   *
   * Drawn as plain text rather than markdown. Thinking is a model talking to
   * itself and is rarely well formed; a heading half way through it would
   * chunk the answer's passages with a passage that is not the answer.
   */
  import { drawer } from "$lib/motion";
  import { hint } from "$lib/hint";
  import { seconds } from "$lib/chat/text";

  interface Props {
    text: string;
    /** Whether it is still thinking. */
    live?: boolean;
    /** How long it thought for, once something followed. */
    ms?: number;
  }

  let { text, live = false, ms }: Props = $props();

  /** Set once somebody presses it, and then it decides rather than `live`. */
  let opened = $state<boolean | null>(null);
  const open = $derived(opened ?? live);

  const label = $derived(
    live ? "Thinking" : ms !== undefined ? `Thought for ${seconds(ms)}` : "Thought about it",
  );
</script>

<div class="thinking">
  <button
    class="head"
    onclick={() => (opened = !open)}
    aria-expanded={open}
    use:hint={open ? "Hide the thinking" : "Show the thinking"}
  >
    <span class="pip" class:working={live} aria-hidden="true"></span>
    <span class="label" class:shimmer={live}>{label}</span>
    <svg class="chevron" class:up={open} width="10" height="10" viewBox="0 0 10 10" fill="none" aria-hidden="true">
      <path d="M2 3.5L5 6.5l3-3" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" />
    </svg>
  </button>

  {#if open}
    <div class="thought sill-scrolls" transition:drawer>
      <p>{text}</p>
    </div>
  {/if}
</div>

<style>
  .thinking {
    align-self: flex-start;
    max-width: 68ch;
    width: 100%;
  }

  .head {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: 0;
    border: 0;
    background: transparent;
    color: var(--text-3);
    font: inherit;
    font-size: var(--text-meta);
    text-align: left;
    cursor: pointer;
    transition: color var(--motion-state) var(--ease);
  }

  .head:hover {
    color: var(--text-2);
  }

  .head:focus-visible {
    outline: none;
    color: var(--text-1);
  }

  .pip {
    width: 4px;
    height: 4px;
    flex: none;
    border-radius: 50%;
    background: var(--accent);
    opacity: 0.7;
  }

  .working {
    animation: pulse var(--motion-pulse) ease-in-out infinite;
  }

  @keyframes pulse {
    0%,
    100% {
      opacity: 0.25;
    }
    50% {
      opacity: 1;
    }
  }

  /* Light sweeping across the word while it is true. */
  .shimmer {
    background: var(--shimmer-ink) 0 0 / 200% 100%;
    -webkit-background-clip: text;
    background-clip: text;
    color: transparent;
    animation: shimmer var(--motion-shimmer) linear infinite;
  }

  @keyframes shimmer {
    to {
      background-position: -200% center;
    }
  }

  .chevron {
    flex: none;
    transition: transform var(--motion-state) var(--ease);
  }

  .up {
    transform: rotate(180deg);
  }

  /*
   * The thinking itself, set in from the line above it and bounded.
   *
   * A hairline on the left rather than a box: it is a margin note to the
   * answer, not a second answer. Twelve lines, then it scrolls.
   */
  .thought {
    margin: var(--space-2) 0 0 var(--space-1);
    padding: 0 0 0 var(--space-3);
    border-left: 1px solid var(--timeline-line);
    max-height: calc(var(--line-body) * 12);
    overflow-y: auto;
  }

  .thought p {
    margin: 0;
    color: var(--text-3);
    font-size: var(--text-meta);
    line-height: var(--line-body);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }

  @media (prefers-reduced-motion: reduce) {
    .working,
    .shimmer {
      animation: none;
      opacity: 1;
    }

    .shimmer {
      background: none;
      color: var(--text-2);
    }
  }
</style>

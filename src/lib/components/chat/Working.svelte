<script lang="ts">
  /**
   * The turn in flight, from the question leaving to the answer landing.
   *
   * The orb moving is what says work is happening, so it stays mounted for
   * the whole turn rather than only for the gap before the first part
   * arrives. One element and one WebGL context: the fluid churns from the
   * keystroke through to the last word, instead of stopping and starting
   * again a quarter of a second in, which is all that gap usually lasts.
   *
   * The words belong to the gap alone. Once parts are arriving the timeline
   * and the prose say what is happening, and "Thinking" under a paragraph
   * being written is a line that has stopped being true.
   *
   * Light sweeps across the words, which reads as activity without a spinner,
   * and the three dots breathe in turn. Both are CSS on elements that exist
   * only while this is on screen, so an idle window carries none of it.
   *
   * No canned status line. "Reading your files..." fits one turn in ten and
   * reads as nonsense on the rest; the timeline says what is actually being
   * read once a tool runs.
   */
  import Orb from "./Orb.svelte";

  interface Props {
    /** Whether parts have started arriving, which is when the words go. */
    writing?: boolean;
  }

  let { writing = false }: Props = $props();
</script>

<p
  class="working"
  class:trailing={writing}
  role="status"
  aria-label={writing ? "Answering" : "Thinking"}
>
  <Orb motion="live" />
  {#if !writing}
    <span class="words" aria-hidden="true">
      Thinking<span class="dots"><span>.</span><span>.</span><span>.</span></span>
    </span>
  {/if}
</p>

<style>
  .working {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    align-self: flex-start;
    margin: 0;
    font-size: var(--text-body);
    font-weight: var(--weight-medium);
  }

  /*
   * Under the answer it is a mark on that answer, not the next thing in the
   * conversation, so it sits closer than the gap between turns puts it.
   */
  .trailing {
    margin-top: calc(var(--space-2) - var(--space-4));
  }

  .words {
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

  .dots span {
    display: inline-block;
    opacity: 0.25;
    animation: breathe var(--motion-pulse) ease-in-out infinite;
  }

  .dots span:nth-child(2) {
    animation-delay: calc(var(--motion-pulse) * 0.14);
  }

  .dots span:nth-child(3) {
    animation-delay: calc(var(--motion-pulse) * 0.28);
  }

  @keyframes breathe {
    0%,
    60%,
    100% {
      opacity: 0.25;
    }
    30% {
      opacity: 1;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .words {
      animation: none;
      background: none;
      color: var(--text-2);
    }

    .dots span {
      animation: none;
      opacity: 1;
    }
  }
</style>

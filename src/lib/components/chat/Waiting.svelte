<script lang="ts">
  /**
   * The wait between the question going and the first thing arriving.
   *
   * The orb moving is what says work is happening; the words say what kind.
   * Light sweeps across them, which reads as activity without a spinner, and
   * the three dots breathe in turn. Both are CSS on elements that exist only
   * while this is on screen, so an idle window carries none of it.
   *
   * No canned status line. "Reading your files..." fits one turn in ten and
   * reads as nonsense on the rest; the timeline says what is actually being
   * read once a tool runs.
   */
  import Orb from "./Orb.svelte";
</script>

<p class="waiting" role="status" aria-label="Thinking">
  <Orb live />
  <span class="words" aria-hidden="true">
    Thinking<span class="dots"><span>.</span><span>.</span><span>.</span></span>
  </span>
</p>

<style>
  .waiting {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    align-self: flex-start;
    margin: 0;
    font-size: var(--text-body);
    font-weight: var(--weight-medium);
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

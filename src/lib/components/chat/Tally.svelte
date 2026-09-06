<script lang="ts">
  /**
   * What the conversation has cost, at the end of the field.
   *
   * One pill: the tokens, the dollars when there are any, and the speed when
   * the model is on this machine. Rust adds up and hands the total over on
   * every done event; while an answer is still arriving the pill ticks on
   * the pieces streamed and breathes to say the figure is not settled yet.
   * The sentence on hover says what was counted and where the price came
   * from, including when no price is known.
   *
   * The one green thing in the chat. The accent means selection and focus;
   * this is a reading, and it is in the colour readings that are going well
   * use everywhere else in Sill.
   */
  import type { AiReady } from "$lib/exthost/commands";
  import type { Live } from "$lib/chat/live";
  import { reading } from "$lib/chat/tally";
  import { hint } from "$lib/hint";

  interface Props {
    live: Live;
    /** Who answers, for the kind and the rate. */
    answersWith: AiReady | null;
  }

  let { live, answersWith }: Props = $props();

  // Recomputed on every piece, because `streamed` moves on every piece; the
  // clock read inside is fresh each time for the same reason.
  const shown = $derived(
    reading(
      live.spent,
      {
        asking: live.asking,
        streamed: live.streamed,
        streamBegan: live.streamBegan,
        now: performance.now(),
      },
      answersWith,
    ),
  );
</script>

{#if shown}
  <span class="tally" class:live={shown.live} use:hint={shown.hint}>
    <span class="dot" aria-hidden="true"></span>
    <span>{shown.tokens}</span>
    {#if shown.cost}
      <span class="sep" aria-hidden="true">&middot;</span>
      <span>{shown.cost}</span>
    {/if}
    {#if shown.rate}
      <span class="sep" aria-hidden="true">&middot;</span>
      <span>{shown.rate}</span>
    {/if}
  </span>
{/if}

<style>
  .tally {
    display: inline-flex;
    align-items: center;
    flex: none;
    gap: var(--space-cozy);
    height: 24px;
    padding: 0 var(--space-2);
    border-radius: var(--radius-pill);
    background: var(--success-fill);
    color: var(--success);
    font-size: var(--text-meta);
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
    cursor: default;
  }

  .dot {
    width: 6px;
    height: 6px;
    border-radius: var(--radius-pill);
    background: var(--success);
  }

  /* Breathing while the figure is an estimate, still while it is the truth. */
  .live .dot {
    animation: breathe var(--motion-shimmer) ease-in-out infinite;
  }

  @keyframes breathe {
    50% {
      opacity: 0.3;
    }
  }

  .sep {
    opacity: 0.5;
  }

  @media (prefers-reduced-motion: reduce) {
    .live .dot {
      animation: none;
    }
  }
</style>

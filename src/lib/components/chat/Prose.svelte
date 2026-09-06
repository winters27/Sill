<script lang="ts">
  /**
   * The words of an answer, revealed at a pace while they arrive.
   *
   * Tokens arrive in clumps; this keeps what arrived as a target and lets
   * the screen catch up to it, one tick every `--motion-reveal`. The pace
   * itself is in `$lib/chat/reveal`; this owns the single timeout.
   *
   * A paragraph that was already finished when it first appeared is drawn
   * whole: a conversation loaded from history must not replay every reply.
   * One that finishes while being watched lands its last characters at the
   * pace it was writing at rather than snapping the tail in.
   */
  import { onMount, untrack } from "svelte";
  import Markdown from "$lib/components/Markdown.svelte";
  import { advance, behind } from "$lib/chat/reveal";
  import { healRunOns } from "$lib/chat/text";

  interface Props {
    text: string;
    /** Whether more is still arriving. */
    live?: boolean;
  }

  let { text, live = false }: Props = $props();

  const target = $derived(healRunOns(text));

  /** Decided once: whether this was being written when it first appeared. */
  const startedLive = untrack(() => live);
  const still =
    typeof window !== "undefined" && window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  const skip = still || !startedLive;

  let shown = $state("");
  let tick = 0;

  /** Read once. `getComputedStyle` forces a style pass; once is free. */
  let pace = 40;
  onMount(() => {
    const raw = getComputedStyle(document.documentElement).getPropertyValue("--motion-reveal");
    const ms = Number.parseFloat(raw);
    if (Number.isFinite(ms) && ms > 0) pace = ms;
  });

  $effect(() => {
    if (skip) {
      shown = target;
      return;
    }

    if (!behind(shown, target)) return;

    // One step per tick. The effect re-runs when `shown` moves, so this
    // advances and then stops on its own once it has caught up.
    tick = window.setTimeout(() => {
      shown = advance(shown, target);
    }, pace);

    return () => window.clearTimeout(tick);
  });
</script>

<div class="prose md">
  <Markdown text={shown} />
</div>

<style>
  /*
   * At least one line tall from the first character, so the row does not
   * grow a line the instant the stream stops and the actions appear.
   */
  .prose {
    min-height: var(--line-body);
  }
</style>

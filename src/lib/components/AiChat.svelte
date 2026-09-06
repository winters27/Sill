<script lang="ts">
  /**
   * A conversation, drawn inside the launcher.
   *
   * The launcher's own field stays the composer, so a follow-up is typed
   * where the question was. Everything drawn here is the same set of
   * components the chat window draws, at the launcher's width: one turn, one
   * timeline, one card, one wait, and the two are the same conversation.
   *
   * ## What this does not own
   *
   * The conversation itself is held in Rust, and the window keeps a copy for
   * drawing. Asking, resuming, forgetting and the events an answer arrives on
   * are all the launcher's, because every one of them also moves the mode,
   * the field or the status line. This draws what it is given and reports
   * the two keystrokes that belong to what is on screen.
   */
  import type { AiReady } from "$lib/exthost/commands";
  import type { Live } from "$lib/chat/live";
  import { textOf } from "$lib/chat/live";
  import type { Shown } from "$lib/chat/parts";
  import { follow } from "$lib/chat/follow";
  import ApprovalCard from "./chat/ApprovalCard.svelte";
  import Opening from "./chat/Opening.svelte";
  import Trouble from "./chat/Trouble.svelte";
  import Turn from "./chat/Turn.svelte";
  import Waiting from "./chat/Waiting.svelte";

  interface Props {
    conversation: Shown[];
    /** The turn in flight: what has arrived, the card, the trouble. */
    live: Live;
    /** Who answers, for the invitation an empty conversation shows. */
    answersWith: AiReady | null;
    /** Yes or no to the card. */
    ondecide: (allowed: boolean) => void;
    /** An example, put in the field rather than sent. */
    onoffer: (question: string) => void;
  }

  let { conversation, live, answersWith, ondecide, onoffer }: Props = $props();

  /**
   * The answer being written, as a turn.
   *
   * Drawn at the end of the same list as the finished turns and keyed by its
   * position, so when it is committed the element that was writing it keeps
   * writing rather than being replaced by one that draws it whole.
   */
  const writing = $derived<Shown | null>(
    live.parts.length
      ? { role: "assistant", text: textOf(live.parts), parts: live.parts, attachments: [] }
      : null,
  );

  const shown = $derived(writing ? [...conversation, writing] : conversation);
</script>

<div class="chat sill-scrolls" use:follow={live.asking}>
  <div class="flow">
    {#if conversation.length === 0 && !live.asking && !writing}
      <Opening {answersWith} {onoffer} />
    {/if}

    {#each shown as turn, at (at)}
      <Turn {turn} live={at === shown.length - 1 && writing !== null} busy={live.asking} />
    {/each}

    {#if live.asked}
      <ApprovalCard asked={live.asked} {ondecide} />
    {/if}

    {#if live.asking && !writing && !live.asked}
      <Waiting />
    {/if}

    {#if live.trouble}
      <Trouble why={live.trouble} />
    {/if}
  </div>
</div>

<style>
  /*
   * A conversation, which reads as a column of paragraphs rather than a list.
   *
   * It scrolls on its own so the field below stays put: a composer that moves
   * down the window as the answer grows is a composer you have to chase.
   */
  .chat {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: var(--space-4) var(--space-4) var(--space-5);
  }

  /* One element holding everything, which is what the follow watches grow. */
  .flow {
    display: flex;
    flex-direction: column;
    /* Wider between turns than inside one, so the conversation reads as
       exchanges rather than as a single column of paragraphs. */
    gap: var(--space-4);
  }
</style>

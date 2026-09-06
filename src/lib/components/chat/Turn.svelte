<script lang="ts">
  /**
   * One turn of a conversation, either side.
   *
   * A question sits right in a bubble of its own and an answer sits left with
   * none. That asymmetry is the point: the question is a few words and reads
   * as a card, the answer is prose and reads as prose, and boxing both makes
   * a long answer into a wall inside a wall. No mark, no name: the answer is
   * the assistant, and a face beside every paragraph is a face nobody looks at.
   *
   * An answer is drawn from its parts in the order they happened: thinking,
   * then what was looked at, then words, then more of any of them. Only the
   * last block of a live turn is live; everything before it is over.
   */
  import type { Shown } from "$lib/chat/parts";
  import { groupParts } from "$lib/chat/parts";
  import Actions from "./Actions.svelte";
  import Prose from "./Prose.svelte";
  import Thinking from "./Thinking.svelte";
  import Timeline from "./Timeline.svelte";

  interface Props {
    turn: Shown;
    /** Whether this answer is still being written. */
    live?: boolean;
    /** Ask again, offered on the last answer only. */
    onagain?: () => void;
    /** Whether a question is in flight, so Again knows to wait. */
    busy?: boolean;
  }

  let { turn, live = false, onagain, busy = false }: Props = $props();

  const blocks = $derived(groupParts(turn.parts));
</script>

{#if turn.role === "user"}
  <article class="turn asked">
    {#if turn.attachments.length}
      <div class="carried">
        {#each turn.attachments as one (one.name)}
          {#if one.kind === "image"}
            <img class="shot" src={one.body} alt={one.name} />
          {:else}
            <span class="paper">{one.name}</span>
          {/if}
        {/each}
      </div>
    {/if}
    {#if turn.text}<p>{turn.text}</p>{/if}
  </article>
{:else}
  <article class="turn said">
    {#each blocks as block, at (block.at)}
      {#if block.kind === "thinking"}
        <Thinking text={block.text} ms={block.ms} live={live && at === blocks.length - 1} />
      {:else if block.kind === "steps"}
        <Timeline steps={block.steps} {live} />
      {:else}
        <Prose text={block.text} live={live && at === blocks.length - 1} />
      {/if}
    {/each}

    {#if !live && turn.text}
      <Actions text={turn.text} {onagain} {busy} />
    {/if}
  </article>
{/if}

<style>
  .turn {
    font-size: var(--text-body);
  }

  .asked {
    align-self: flex-end;
    max-width: 72%;
    padding: var(--space-2) var(--space-3);
    border-radius: var(--radius-lg) var(--radius-lg) var(--radius-sm) var(--radius-lg);
    background: var(--accent-fill);
    box-shadow: var(--ring-accent-faint);
  }

  .asked p {
    margin: 0;
    color: var(--text-1);
    line-height: 1.55;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }

  .said {
    align-self: flex-start;
    max-width: 74ch;
    width: 100%;
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    color: var(--text-1);
  }

  /* What was handed over with a question, drawn inside its bubble. */
  .carried {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
    margin-bottom: var(--space-2);
  }

  .carried:last-child {
    margin-bottom: 0;
  }

  .shot {
    max-width: 220px;
    max-height: 160px;
    border-radius: var(--radius-md);
    display: block;
  }

  .paper {
    padding: var(--space-half) var(--space-2);
    border-radius: var(--radius-sm);
    background: var(--fill-2);
    color: var(--text-1);
    font-family: var(--font-mono);
    font-size: var(--text-micro);
  }
</style>

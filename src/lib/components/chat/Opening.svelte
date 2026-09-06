<script lang="ts">
  /**
   * An empty conversation, which says what this is and what it can reach.
   *
   * Not a greeting. Nothing anywhere else says the model can read this
   * machine, so somebody who does not already know asks it the questions
   * they would ask any chat window and never finds out. Four openers, each
   * needing a different tool, each filling the field rather than sending:
   * an example that sends itself spends money on a question somebody was
   * only reading.
   *
   * Two shapes. In the launcher it sits left, where the first answer will,
   * so nothing moves when one arrives. In the chat window it is a stage:
   * centred in the pane with the composer waiting beneath, which reads as a
   * place to begin rather than a paragraph in the corner.
   */
  import AiMark from "$lib/components/settings/AiMark.svelte";
  import type { AiReady } from "$lib/exthost/commands";
  import type { StepIconName } from "$lib/chat/verbs";
  import Orb from "./Orb.svelte";
  import StepIcon from "./StepIcon.svelte";

  interface Props {
    answersWith: AiReady | null;
    /** An example, put in the field rather than sent. */
    onoffer: (question: string) => void;
    /** Where to go when nothing is set up. Absent, the sentence stands alone. */
    onsetup?: () => void;
    /** The chat window's centred stage rather than the launcher's corner. */
    stage?: boolean;
  }

  let { answersWith, onoffer, onsetup, stage = false }: Props = $props();

  interface Opener {
    question: string;
    reaches: string;
    icon: StepIconName;
  }

  const OPENERS: Opener[] = [
    { question: "What windows do I have open?", reaches: "Looks at what is open", icon: "window" },
    { question: "What did I copy earlier?", reaches: "Reads your clipboard", icon: "clipboard" },
    {
      question: "Find the largest files in my Downloads folder",
      reaches: "Searches your files",
      icon: "search",
    },
    { question: "What is my volume set to?", reaches: "Checks this machine", icon: "machine" },
  ];
</script>

<div class="opening" class:stage>
  <div class="lead">
    <Orb size={stage ? "hero" : "inline"} />
    <div class="words">
      {#if answersWith?.ready}
        <p class="ask">
          <AiMark name={answersWith.id} size={stage ? 18 : 15} />
          <span>Ask {answersWith.model || answersWith.name} anything</span>
        </p>
      {:else}
        <p class="ask">Nothing is set up to answer yet</p>
      {/if}
      <p class="reach">
        It can look through this machine to answer: what is installed and open,
        what you have copied or selected, a file or a folder, and what is on
        screen. Anything that changes something stops to ask you first.
      </p>
    </div>
  </div>

  {#if answersWith?.ready}
    <div class="openers">
      {#each OPENERS as one (one.question)}
        <button class="opener" onclick={() => onoffer(one.question)}>
          <span class="tile"><StepIcon name={one.icon} /></span>
          <span class="text">
            <span class="question">{one.question}</span>
            <span class="reaches">{one.reaches}</span>
          </span>
        </button>
      {/each}
    </div>
  {:else if onsetup}
    <button class="setup" onclick={() => onsetup?.()}>Set up AI Chat</button>
  {/if}
</div>

<style>
  .opening {
    align-self: flex-start;
    max-width: 62ch;
    width: 100%;
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    padding-top: var(--space-2);
  }

  .lead {
    display: flex;
    align-items: center;
    gap: var(--space-3);
  }

  .words {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    min-width: 0;
  }

  .ask {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    margin: 0;
    color: var(--text-1);
    font-size: var(--text-heading);
    font-weight: var(--weight-strong);
  }

  .reach {
    margin: 0;
    color: var(--text-2);
    font-size: var(--text-body);
    line-height: 1.6;
  }

  /*
   * The stage: everything centred, the orb above the words, the openers as
   * a two-by-two under them. `margin: auto` on both axes is what centres it
   * in whatever room the pane has.
   */
  .stage {
    align-self: center;
    max-width: 74ch;
    margin: auto 0;
    gap: var(--space-5);
    align-items: center;
    text-align: center;
    padding: var(--space-6) 0;
  }

  .stage .lead {
    flex-direction: column;
    gap: var(--space-4);
  }

  .stage .words {
    align-items: center;
  }

  .stage .ask {
    justify-content: center;
    font-size: var(--text-title);
  }

  .stage .reach {
    max-width: 52ch;
  }

  .openers {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(200px, 1fr));
    gap: var(--space-2);
    width: 100%;
    text-align: left;
  }

  .stage .openers {
    grid-template-columns: 1fr 1fr;
    max-width: 62ch;
  }

  /*
   * An example, which fills the field rather than sending it.
   *
   * Drawn as something pressable rather than as a bullet, because it is; and
   * quiet rather than accented, because four accented cards in an empty
   * window read as the main event when they are a way in.
   */
  .opener {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    min-width: 0;
    padding: var(--space-2) var(--space-3) var(--space-2) var(--space-2);
    border: 0;
    border-radius: var(--radius-md);
    background: var(--fill-1);
    box-shadow: var(--ring);
    color: var(--text-2);
    font: inherit;
    text-align: left;
    cursor: pointer;
    transition:
      background-color var(--motion-state) var(--ease),
      color var(--motion-state) var(--ease);
  }

  .opener:hover {
    background: var(--fill-2);
    color: var(--text-1);
  }

  .opener:focus-visible {
    outline: none;
    box-shadow: var(--ring-accent);
  }

  .tile {
    display: grid;
    place-items: center;
    flex: none;
    width: var(--icon-tile);
    height: var(--icon-tile);
    border-radius: var(--radius-sm);
    background: var(--fill-2);
    color: var(--text-2);
  }

  .tile :global(svg) {
    width: var(--glyph-sm);
    height: var(--glyph-sm);
  }

  .text {
    display: flex;
    flex-direction: column;
    gap: var(--space-hair);
    min-width: 0;
  }

  .question {
    color: var(--text-1);
    font-size: var(--text-meta);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .reaches {
    color: var(--text-3);
    font-size: var(--text-micro);
  }

  .setup {
    align-self: flex-start;
    padding: var(--space-2) var(--space-3);
    border: 0;
    border-radius: var(--radius-md);
    background: var(--accent-fill);
    color: var(--accent);
    font: inherit;
    font-size: var(--text-meta);
    cursor: pointer;
  }

  .stage .setup {
    align-self: center;
  }
</style>

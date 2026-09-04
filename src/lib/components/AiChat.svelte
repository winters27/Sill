<script module lang="ts">
  import type { AiStep } from "$lib/exthost/commands";

  /**
   * One turn as the window draws it: what was said, and what was looked at to
   * say it.
   *
   * The steps belong to the turn rather than to the conversation, because
   * that is what they are about. Held for the moment only: they are
   * provenance, and a conversation reopened tomorrow is the answer rather
   * than the working.
   */
  export interface Shown {
    role: string;
    text: string;
    steps: AiStep[];
  }
</script>

<script lang="ts">
  /**
   * A conversation, with each side on its own.
   *
   * A question sits right in a bubble of its own and an answer sits left with
   * none. That asymmetry is the point rather than an oversight: the question
   * is a few words and reads as a card, the answer is prose and reads as
   * prose, and boxing both makes a long answer into a wall inside a wall. The
   * launcher's own field stays the composer, so a follow-up is typed where
   * the question was.
   *
   * ## What this does not own
   *
   * The conversation itself is held in Rust, and the window keeps a copy for
   * drawing. Asking, resuming, forgetting and the five events an answer
   * arrives on are all the launcher's, because every one of them also moves
   * the mode, the field or the status line. This draws what it is given and
   * reports the two keystrokes that belong to what is on screen.
   */
  import AiMark from "$lib/components/settings/AiMark.svelte";
  import Markdown from "$lib/components/Markdown.svelte";
  import Steps from "$lib/components/Steps.svelte";
  import type { AiAsking, AiReady } from "$lib/exthost/commands";

  interface Props {
    conversation: Shown[];
    /** The answer being written right now, before it becomes a turn. */
    answering: string;
    /** Whether a question is in flight. */
    asking: boolean;
    /** What the model wants to do, while nobody has said yes or no. */
    asked: AiAsking | null;
    /** What the model has looked at during the turn in flight. */
    steps: AiStep[];
    /** Who answers, for the invitation an empty conversation shows. */
    answersWith: AiReady | null;
    /** Yes or no to the card. */
    ondecide: (allowed: boolean) => void;
    /** An example, put in the field rather than sent. */
    onoffer: (question: string) => void;
  }

  let {
    conversation,
    answering,
    asking,
    asked,
    steps,
    answersWith,
    ondecide,
    onoffer,
  }: Props = $props();

  /**
   * What to ask, offered to an empty conversation.
   *
   * Not decoration. Nothing anywhere else says the model can read this
   * machine, so somebody who does not already know asks it the questions they
   * would ask any chat window and never finds out. Each of these needs a tool
   * to answer, and each names a different one.
   */
  const OPENERS = [
    "What windows do I have open?",
    "What did I copy earlier?",
    "Find the largest files in my Downloads folder",
    "What is my volume set to?",
  ];

  /** The scrolling column, so it can be kept at the bottom as text arrives. */
  let chatScroll = $state<HTMLDivElement | null>(null);

  /*
   * Kept at the bottom while an answer is being written.
   *
   * Only while it is being written, and only if the reader is already at the
   * bottom: yanking somebody back down while they are reading what was said
   * earlier is worse than letting the new text arrive out of sight.
   */
  $effect(() => {
    answering;
    conversation;

    const box = chatScroll;
    if (!box) return;

    const nearBottom = box.scrollHeight - box.scrollTop - box.clientHeight < 80;
    if (nearBottom) box.scrollTop = box.scrollHeight;
  });
</script>

<div class="chat sill-scrolls" bind:this={chatScroll}>
  <!--
    An empty conversation says what this is and what it can reach.

    The launcher's own answer to a blank window: not a greeting, but the
    four questions that are worth asking here and nowhere else. They are
    the only place the tools are visible before one runs.
  -->
  {#if conversation.length === 0 && !asking && !answering}
    <div class="opening">
      {#if answersWith?.ready}
        <!-- An invitation rather than a label. The crumb two lines above
             already names the model; saying it again as a heading reads as
             the same fact twice, and as part of a sentence it does not. -->
        <p class="lead">
          <AiMark name={answersWith.id} size={15} />
          <span>Ask {answersWith.model || answersWith.name} anything</span>
        </p>
      {/if}
      <p class="reach">
        It can look through this machine to answer: what is installed and
        open, what you have copied or selected, a file or a folder, and
        what is on screen.
      </p>
      <div class="openers">
        {#each OPENERS as opener (opener)}
          <button class="opener" onclick={() => onoffer(opener)}>{opener}</button>
        {/each}
      </div>
    </div>
  {/if}

  {#each conversation as turn, at (at)}
    {#if turn.role === "user"}
      <article class="turn asked"><p>{turn.text}</p></article>
    {:else}
      <!--
        What was looked at, above the answer it produced.

        One line per tool with what it was used on, because ten lookups
        that all read "Searched" read as a stutter and say nothing about
        what was searched for. It stays after the answer arrives: knowing
        that a question about your machine was answered by reading your
        clipboard is part of the answer.
      -->
      <Steps steps={turn.steps} />
      <article class="turn said md"><Markdown text={turn.text} /></article>
    {/if}
  {/each}

  {#if asking}
    <Steps {steps} live />
  {/if}

  {#if asked}
    <!--
      What it wants to do, and the two keys that answer.

      Enter and Escape rather than buttons, because the field already has
      focus and reaching for a mouse to answer a question about your own
      files is the wrong shape. The keys are drawn anyway: a control that
      exists only as a keystroke nobody was told about is a control nobody
      uses.
    -->
    <div class="permission">
      <p class="wants">{asked.title}</p>
      <p class="subject">{asked.subject}</p>
      <p class="touches">This {asked.touches}.</p>
      <div class="answers">
        <button class="allow" onclick={() => ondecide(true)}>
          <span class="sill-key">Enter</span> Do it
        </button>
        <button class="refuse" onclick={() => ondecide(false)}>
          <span class="sill-key">Esc</span> Not now
        </button>
      </div>
    </div>
  {/if}

  {#if answering}
    <article class="turn said md"><Markdown text={answering} /></article>
  {:else if asking && !asked}
    <!-- Something between pressing Tab and the first token arriving,
         because a blank panel reads as nothing having happened. -->
    <p class="thinking">Thinking<span class="dots" aria-hidden="true"></span></p>
  {/if}
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
    display: flex;
    flex-direction: column;
    /* Wider between turns than inside one, so the conversation reads as
       exchanges rather than as a single column of paragraphs. */
    gap: var(--space-4);
    padding: var(--space-4) var(--space-4) var(--space-5);
  }

  .turn {
    font-size: var(--text-body);
  }

  /*
   * The question, to the right and in a ground of its own.
   *
   * Short by nature, so it can afford a bubble and gains from one: it is the
   * only thing on screen that somebody wrote themselves, and finding it again
   * in a long conversation is how you remember what you asked.
   */
  .asked {
    align-self: flex-end;
    max-width: 78%;
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

  /*
   * The answer, to the left and unboxed.
   *
   * No ground, because prose in a box is a wall inside a wall, and the width
   * is capped where a line stops being comfortable to read rather than at the
   * window edge.
   */
  .said {
    align-self: flex-start;
    max-width: 68ch;
    width: 100%;
    color: var(--text-1);
  }

  /*
   * What was asked, set apart from what was answered.
   *
   * Quieter rather than boxed. A question is a heading for the answer under
   * it, and drawing a bubble round each turn would make a short exchange look
   * like a chat application rather than a launcher.
   */
  .asked p {
    color: var(--text-2);
  }

  /*
   * The empty conversation.
   *
   * Left aligned with the answers rather than centred, because it sits where
   * the first answer will and centring it would move everything the moment one
   * arrives.
   */
  .opening {
    align-self: flex-start;
    max-width: 62ch;
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    padding-top: var(--space-2);
  }

  .lead {
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

  .openers {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
  }

  /*
   * An example, which fills the field rather than sending it.
   *
   * Drawn as something pressable rather than as a bullet, because it is; and
   * quiet rather than accented, because four accented chips in an empty window
   * read as the main event when they are a way in.
   */
  .opener {
    padding: var(--space-1) var(--space-3);
    border: 0;
    border-radius: var(--radius-pill);
    background: var(--fill-1);
    box-shadow: var(--ring);
    color: var(--text-2);
    font: inherit;
    font-size: var(--text-meta);
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

  /*
   * The card that asks before something changes.
   *
   * The one thing in a conversation that is not a message, so it is the one
   * thing with a ground and an outline. It sits where the next answer would,
   * because that is where somebody is already looking.
   */
  .permission {
    align-self: flex-start;
    max-width: 62ch;
    width: 100%;
    padding: var(--space-3);
    border-radius: var(--radius-lg);
    background: var(--fill-1);
    box-shadow: var(--ring-accent-faint);
  }

  .wants {
    margin: 0;
    color: var(--accent);
    font-size: var(--text-meta);
    font-weight: var(--weight-strong);
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }

  /* What it acts on, which is the line somebody actually decides on. */
  .subject {
    margin: var(--space-1) 0 0;
    color: var(--text-1);
    font-size: var(--text-body);
    line-height: 1.5;
    overflow-wrap: anywhere;
  }

  .touches {
    margin: var(--space-1) 0 var(--space-3);
    color: var(--text-2);
    font-size: var(--text-meta);
  }

  .answers {
    display: flex;
    gap: var(--space-2);
  }

  .answers button {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-1) var(--space-3);
    border: 0;
    border-radius: var(--radius-sm);
    background: var(--fill-2);
    color: var(--text-2);
    font: inherit;
    font-size: var(--text-meta);
    cursor: pointer;
    transition:
      background-color var(--motion-state) var(--ease),
      color var(--motion-state) var(--ease);
  }

  .answers button:hover {
    color: var(--text-1);
  }

  /*
   * The affirmative takes the accent, and only the affirmative.
   *
   * Two coloured buttons is two things shouting; a refusal that looks like a
   * warning also reads as the dangerous one, which is backwards.
   */
  .allow {
    background: var(--accent-fill);
    color: var(--accent);
  }

  .allow:hover {
    background: var(--accent-fill-strong);
  }

  .answers button:focus-visible {
    outline: none;
    box-shadow: var(--ring-accent);
  }

  /*
   * The wait, said without a spinner.
   *
   * A launcher is meant to feel instant and a spinner advertises that it is
   * not. Three dots that fill in say the same thing while making the wait a
   * detail rather than the subject.
   */
  .thinking {
    align-self: flex-start;
    margin: 0;
    color: var(--text-2);
    font-size: var(--text-body);
  }

  .dots::after {
    content: "";
    animation: thinking var(--motion-pulse) steps(4, end) infinite;
  }

  @keyframes thinking {
    0% {
      content: "";
    }
    25% {
      content: ".";
    }
    50% {
      content: "..";
    }
    75% {
      content: "...";
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .dots::after {
      animation: none;
      content: "…";
    }
  }
</style>

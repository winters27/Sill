<script lang="ts">
  /**
   * The box a question is typed into, in the chat window.
   *
   * One raised object: the words, what is attached, the paperclip, who
   * answers, the key that sends, and the button that sends, all on one
   * bevelled card that floats in the pane. It is the one place this window
   * spends its depth; everything around it stays flat so the card reads as
   * the thing you reach for.
   *
   * Enter sends, Shift and Enter writes a line: the opposite of the launcher,
   * where the field is one line and Enter is the only thing it can mean. While
   * a card is up, Enter and Escape answer the card instead; while an answer is
   * arriving, Escape stops it.
   *
   * Grows to fit what is in it, measured here rather than left to
   * `field-sizing`, which is new enough that whether it works depends on the
   * WebView2 runtime installed. Height is cleared before it is read, because
   * `scrollHeight` on an element with a height already set reports that
   * height rather than the content.
   */
  import AiMark from "$lib/components/settings/AiMark.svelte";
  import Tally from "$lib/components/chat/Tally.svelte";
  import type { AiAsking, AiAttached, AiReady } from "$lib/exthost/commands";
  import type { Live } from "$lib/chat/live";
  import { hint } from "$lib/hint";

  interface Props {
    draft: string;
    /** What is waiting to go with the next question. */
    carrying: AiAttached[];
    /** The box itself, so the page can focus it. */
    field?: HTMLTextAreaElement | null;
    /** Whether a question is in flight. */
    asking: boolean;
    /** The card up, if one is, since Enter answers it before it sends. */
    asked: AiAsking | null;
    /** Whether this is the first question, for the placeholder. */
    first: boolean;
    /** Who answers, for the chip that says so. */
    answersWith: AiReady | null;
    /**
     * The turn in flight and the conversation's total, for the pill that
     * counts what it has cost. Optional, because the box does not need it to
     * be a box.
     */
    live?: Live;
    onsend: () => void;
    onstop: () => void;
    onpick: () => void;
    /** A picture pasted straight in. */
    onpaste: (event: ClipboardEvent) => void;
    ondecide: (allowed: boolean) => void;
    /** Where to change who answers. */
    onsettings: () => void;
  }

  let {
    draft = $bindable(""),
    carrying = $bindable([]),
    field = $bindable(null),
    asking,
    asked,
    first,
    answersWith,
    live,
    onsend,
    onstop,
    onpick,
    onpaste,
    ondecide,
    onsettings,
  }: Props = $props();

  /** As far as it grows before it starts scrolling instead. */
  const GROWS_TO = 200;

  function fit(box: HTMLTextAreaElement | null) {
    if (!box) return;
    box.style.height = "auto";
    box.style.height = `${Math.min(box.scrollHeight, GROWS_TO)}px`;
  }

  // Whatever changes what is in it changes how tall it is: typing, sending,
  // and putting a question back to ask again.
  $effect(() => {
    draft;
    fit(field);
  });

  function onKey(event: KeyboardEvent) {
    if (asked) {
      if (event.key === "Enter") {
        event.preventDefault();
        ondecide(true);
        return;
      }
      if (event.key === "Escape") {
        event.preventDefault();
        ondecide(false);
        return;
      }
    }

    if (asking && event.key === "Escape") {
      event.preventDefault();
      onstop();
      return;
    }

    if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      onsend();
    }
  }

  function drop(name: string) {
    carrying = carrying.filter((one) => one.name !== name);
  }

  /** A size somebody would say out loud. */
  function size(bytes: number): string {
    if (bytes >= 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
    if (bytes >= 1024) return `${Math.round(bytes / 1024)} KB`;
    return `${bytes} bytes`;
  }

  const whom = $derived(
    answersWith?.ready ? answersWith.model || answersWith.name : "Set up AI Chat",
  );
</script>

<div class="dock">
  <div class="composer" class:busy={asking}>
    {#if carrying.length}
      <div class="waiting">
        {#each carrying as one (one.name)}
          <span class="chip">
            {#if one.kind === "image"}
              <img class="thumb" src={one.body} alt="" />
            {/if}
            <span class="chip-name">{one.name}</span>
            <span class="chip-size">{size(one.bytes)}</span>
            <button class="chip-drop" aria-label={`Remove ${one.name}`} onclick={() => drop(one.name)}
              >&times;</button
            >
          </span>
        {/each}
      </div>
    {/if}

    <textarea
      bind:this={field}
      bind:value={draft}
      onkeydown={onKey}
      onpaste={onpaste}
      placeholder={asked
        ? "Answer above first…"
        : asking
          ? "Waiting for the answer…"
          : first
            ? "Ask anything…"
            : "Ask a follow-up…"}
      rows="1"
      spellcheck="false"
      aria-label="Ask"
    ></textarea>

    <div class="tools">
      <button class="round attach" onclick={onpick} aria-label="Attach a file" use:hint={"Attach a file"}>
        <svg width="15" height="15" viewBox="0 0 24 24" fill="none" aria-hidden="true">
          <path
            d="M21 11.5l-8.5 8.5a5.5 5.5 0 01-7.8-7.8l8.7-8.7a3.7 3.7 0 015.2 5.2l-8.6 8.6a1.8 1.8 0 01-2.6-2.6l7.9-7.9"
            stroke="currentColor"
            stroke-width="1.7"
            stroke-linecap="round"
            stroke-linejoin="round"
          />
        </svg>
      </button>

      <!--
        Who answers, said where the question is typed.

        The mark carries the service and the model is the half that changes,
        so the chip says only the model. Pressing it goes to where that is
        chosen, which is the same place the launcher's own chip goes.
      -->
      <button
        class="whom"
        class:unset={!answersWith?.ready}
        onclick={onsettings}
        use:hint={answersWith?.ready ? "Change who answers" : (answersWith?.whyNot ?? "")}
      >
        {#if answersWith?.ready}
          <AiMark name={answersWith.id} size={12} />
        {/if}
        <span>{whom}</span>
      </button>

      <!--
        What the conversation has cost, at the far end of the row from the
        attach button, where the eye goes after the answer and before the
        next question.
      -->
      {#if live}
        <span class="grow"></span>
        <Tally {live} {answersWith} />
      {/if}

      <span class="key-hint" aria-hidden="true">
        {#if asking}
          <span class="sill-key">Esc</span> stops
        {:else}
          <span class="sill-key">Enter</span> sends
        {/if}
      </span>

      {#if asking}
        <!--
          Stopping keeps what has arrived, so it is a plain control rather than
          a destructive one. A square inside, which is what stop has meant on
          every machine since tape.
        -->
        <button class="round stop" onclick={onstop} aria-label="Stop" use:hint={"Stop"}>
          <svg width="11" height="11" viewBox="0 0 12 12" aria-hidden="true">
            <rect x="1.5" y="1.5" width="9" height="9" rx="1.5" fill="currentColor" />
          </svg>
        </button>
      {:else}
        <button
          class="round send"
          onclick={onsend}
          disabled={!draft.trim() && carrying.length === 0}
          aria-label="Send"
          use:hint={"Send"}
        >
          <svg width="15" height="15" viewBox="0 0 16 16" fill="none" aria-hidden="true">
            <path
              d="M8 13V3M8 3L3.5 7.5M8 3l4.5 4.5"
              stroke="currentColor"
              stroke-width="1.9"
              stroke-linecap="round"
              stroke-linejoin="round"
            />
          </svg>
        </button>
      {/if}
    </div>
  </div>
</div>

<style>
  /* Room around the card, so it floats in the pane rather than sitting on its edge. */
  .dock {
    flex: none;
    display: flex;
    justify-content: center;
    padding: var(--space-3) var(--space-6) var(--space-5);
  }

  /*
   * The one bevelled object in the window.
   *
   * Sheen over the lightest fill and the hero bevel: the same recipe as a
   * raised tile in the launcher, so the two windows share one idea of what
   * is raised. Nothing else in the pane carries a bevel, which is what
   * makes this one read.
   */
  .composer {
    width: min(74ch, 100%);
    display: flex;
    flex-direction: column;
    border-radius: var(--radius-lg);
    /* The panel tint, not a fill: it lifts off the glass without turning
       into a slab on it. The tile bevel catches the light on two edges and
       the short shadow gives it contact; the hero bevel's brighter highlight
       read as a hard surface here. */
    background: var(--tint-panel);
    box-shadow: var(--bevel-tile), var(--elevation-2);
    transition: box-shadow var(--motion-state) var(--ease);
  }

  /* Focus lifts the card a shade rather than ringing it: a ring on the one
     object that is always focused would be a permanent frame. */
  .composer:focus-within {
    box-shadow: var(--bevel-tile), var(--elevation-2), var(--ring-fill-soft);
  }

  /* What is waiting to go with the next question, on the card's top edge. */
  .waiting {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
    padding: var(--space-3) var(--space-3) 0;
  }

  .chip {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-snug) var(--space-1) var(--space-snug) var(--space-2);
    border-radius: var(--radius-pill);
    background: var(--fill-2);
    font-size: var(--text-meta);
  }

  .thumb {
    width: var(--icon-tile-sm);
    height: var(--icon-tile-sm);
    border-radius: var(--radius-sm);
    object-fit: cover;
  }

  .chip-name {
    color: var(--text-1);
    max-width: 24ch;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .chip-size {
    color: var(--text-3);
    font-size: var(--text-micro);
  }

  .chip-drop {
    width: 18px;
    height: 18px;
    display: grid;
    place-items: center;
    border: 0;
    border-radius: 50%;
    background: transparent;
    color: var(--text-3);
    font-size: var(--text-body);
    line-height: 1;
    cursor: pointer;
  }

  .chip-drop:hover {
    background: var(--fill-3);
    color: var(--text-1);
  }

  /* Borderless on the card: the card is the box. */
  textarea {
    width: 100%;
    box-sizing: border-box;
    min-height: 38px;
    max-height: 200px;
    overflow-y: auto;
    scrollbar-width: thin;
    scrollbar-color: var(--scrollbar-thumb) transparent;
    padding: var(--space-3) var(--space-4) var(--space-1);
    border: 0;
    background: transparent;
    color: var(--text-1);
    font: inherit;
    font-size: var(--text-body);
    line-height: 1.5;
    resize: none;
    outline: none;
  }

  textarea::placeholder {
    color: var(--text-3);
  }

  .tools {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-1) var(--space-2) var(--space-2);
  }

  .round {
    flex: none;
    width: var(--control-height);
    height: var(--control-height);
    display: grid;
    place-items: center;
    border: 0;
    border-radius: 50%;
    cursor: pointer;
    transition:
      background-color var(--motion-state) var(--ease),
      color var(--motion-state) var(--ease),
      opacity var(--motion-state) var(--ease);
  }

  .round:focus-visible {
    outline: none;
    box-shadow: var(--focus-ring);
  }

  .attach {
    background: transparent;
    color: var(--text-2);
  }

  .attach:hover {
    background: var(--fill-2);
    color: var(--text-1);
  }

  .whom {
    display: inline-flex;
    align-items: center;
    gap: var(--space-cozy);
    height: 24px;
    padding: 0 var(--space-2) 0 var(--space-cozy);
    border: 0;
    border-radius: var(--radius-pill);
    background: transparent;
    box-shadow: var(--ring-fill-soft);
    color: var(--text-2);
    font: inherit;
    font-size: var(--text-label);
    cursor: pointer;
    transition:
      background-color var(--motion-state) var(--ease),
      color var(--motion-state) var(--ease);
  }

  .whom:hover {
    background: var(--fill-2);
    color: var(--text-1);
  }

  .whom:focus-visible {
    outline: none;
    box-shadow: var(--ring-accent);
  }

  /* Nothing set up is the one state the chip should draw attention to. */
  .unset {
    padding-left: var(--space-2);
    color: var(--accent);
  }

  /* Takes the slack ahead of the pill, so the pill, the key and the button
     sit together at the end. */
  .grow {
    flex: 1;
  }

  .key-hint {
    margin-left: auto;
    display: inline-flex;
    align-items: center;
    gap: var(--space-cozy);
    color: var(--text-3);
    font-size: var(--text-label);
  }

  /* Stopping keeps what has arrived, so it is not painted as danger. */
  .stop {
    background: var(--fill-3);
    color: var(--text-1);
  }

  .stop:hover {
    background: var(--hairline-strong);
  }

  .send {
    background: var(--accent);
    color: var(--core-background);
  }

  .send:hover:not(:disabled) {
    background: var(--accent-hover);
  }

  /* Nothing to send reads as nothing to press. */
  .send:disabled {
    background: var(--fill-2);
    color: var(--text-3);
    cursor: default;
  }
</style>

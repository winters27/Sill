<script lang="ts">
  /**
   * What the model wants to do, and the two keys that answer.
   *
   * Enter and Escape rather than buttons alone, because the field already
   * has focus and reaching for a mouse to answer a question about your own
   * files is the wrong shape. The keys are drawn anyway: a control that
   * exists only as a keystroke nobody was told about is a control nobody
   * uses. The keys themselves are handled by whichever surface owns the
   * field; this draws the card and takes the click.
   *
   * The one thing in a conversation that is not a message, so it is the one
   * thing with a ground and an outline. It sits where the next answer would,
   * because that is where somebody is already looking.
   */
  import type { AiAsking } from "$lib/exthost/commands";
  import { SETTLES_MS, settled, shownAt } from "$lib/approval";

  interface Props {
    asked: AiAsking;
    ondecide: (allowed: boolean) => void;
  }

  let { asked, ondecide }: Props = $props();

  /*
   * Allow waits until the card has been on screen long enough to be read.
   *
   * Enter is held to the same pause (see `$lib/approval`), and a key that
   * does nothing with no visible reason reads as a broken card. Drawn as not
   * yet pressable instead, so the pause is seen rather than guessed at.
   *
   * One timeout per card, for well under a second, and only while a card is
   * up. Nothing runs when there is no card.
   */
  let ready = $state(false);

  $effect(() => {
    const card = asked;
    const now = performance.now();
    shownAt(card, now);

    if (settled(card, now)) {
      ready = true;
      return;
    }

    ready = false;
    const wait = setTimeout(() => (ready = true), SETTLES_MS);
    return () => clearTimeout(wait);
  });
</script>

<div class="permission sill-glaze sill-glaze-card" role="group" aria-label={asked.title}>
  <p class="wants">{asked.title}</p>
  <p class="subject">{asked.subject}</p>
  <p class="touches">This {asked.touches}.</p>
  <!--
    Said out loud when the stronger gate could not run. Without it a
    keypress and a fingerprint look the same from here.
  -->
  {#if asked.instead}
    <p class="instead">{asked.instead}, so pressing Enter is all this asks for.</p>
  {/if}
  <!-- A yes that is remembered says so, where the yes is given. -->
  {#if asked.lasting}
    <p class="instead">{asked.lasting}</p>
  {/if}
  <div class="answers">
    <button class="allow" disabled={!ready} onclick={() => ondecide(true)}>
      <span class="sill-key">Enter</span> {asked.lasting ? "Allow" : "Do it"}
    </button>
    <button class="refuse" onclick={() => ondecide(false)}>
      <span class="sill-key">Esc</span> Not now
    </button>
  </div>
</div>

<style>
  /* The popover's material at card size, in the flow of the chat: the base
     is the menu's, the rim and highlight are the card glaze's, and a short
     shadow gives it contact rather than the faint accent ring it wore. */
  .permission {
    align-self: flex-start;
    max-width: 62ch;
    width: 100%;
    padding: var(--space-3);
    --glaze-base: var(--menu-base);
    box-shadow: var(--menu-edge), var(--elevation-2);
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

  /* Quieter than what it touches, which is what the decision is about. */
  .instead {
    margin: calc(var(--space-3) * -1) 0 var(--space-3);
    color: var(--text-3);
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
   * The affirmative takes the accent, and only the affirmative. Two coloured
   * buttons is two things shouting; a refusal that looks like a warning also
   * reads as the dangerous one, which is backwards.
   */
  .allow {
    background: var(--accent-fill);
    color: var(--accent);
  }

  .allow:hover {
    background: var(--accent-fill-strong);
  }

  /* Not yet: the card has only just arrived. Quieter, not hidden, so the
     button does not jump when it becomes pressable. */
  .allow:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .answers button:focus-visible {
    outline: none;
    box-shadow: var(--ring-accent);
  }
</style>

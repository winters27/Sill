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

  interface Props {
    asked: AiAsking;
    ondecide: (allowed: boolean) => void;
  }

  let { asked, ondecide }: Props = $props();
</script>

<div class="permission" role="group" aria-label={asked.title}>
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
  <div class="answers">
    <button class="allow" onclick={() => ondecide(true)}>
      <span class="sill-key">Enter</span> Do it
    </button>
    <button class="refuse" onclick={() => ondecide(false)}>
      <span class="sill-key">Esc</span> Not now
    </button>
  </div>
</div>

<style>
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

  .answers button:focus-visible {
    outline: none;
    box-shadow: var(--ring-accent);
  }
</style>

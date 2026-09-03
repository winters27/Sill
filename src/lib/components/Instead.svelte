<script lang="ts">
  /**
   * The one empty, loading and failed state, for every view that has any.
   *
   * There were three designs across eleven views. The mark-and-headline
   * recipe in the launcher, a bare centred paragraph in the clipboard and the
   * action panel, and one class doing all three jobs in the extension store,
   * where a failure and an empty shelf were the same grey sentence in the
   * same place and nothing on screen said which had happened.
   *
   * A class anybody can apply gets half-copied; a component cannot be. This
   * carries the layout, the mark, the type and the three tones, and every
   * view hands it words.
   *
   * ## Two densities, one recipe
   *
   * `pane` is a view whose whole body is this: the root list with no matches,
   * the store still loading, the extension window before its first render. It
   * gets the mark, room around it and a centred column.
   *
   * `inline` is a state inside something else: a paragraph where a settings
   * card's rows would be, the sidebar of past conversations, the action panel.
   * Same type, same colours, same words, no mark and no centring, because a
   * 132px centred column inside a settings card reads as a hole in the panel
   * rather than as an answer.
   *
   * The density is about the space, never about which tone it is. A failure
   * in a settings card is still a failure and still says so.
   *
   * ## No spinner, anywhere
   *
   * The launcher is meant to feel instant and a spinner advertises that it is
   * not. A line of text says the same thing without making the wait the
   * subject of the screen. That was already the rule the launcher followed and
   * it is now the rule the store follows too.
   */
  import type { Snippet } from "svelte";
  import type { Standing } from "$lib/instead";

  interface Props {
    /**
     * Which of the three this is, from `standing()` in `$lib/instead`.
     *
     * `content` is accepted and draws nothing, so a caller can hand this its
     * standing without a second test around it.
     */
    tone: Standing;
    /** The one line somebody reads. */
    headline: string;
    /** The sentence under it, when there is something useful to add. */
    hint?: string;
    /**
     * The same line where the sentence has something in it that is not prose.
     *
     * The quicklinks panel shows a worked example with the placeholder in
     * `<code>`, which is the one thing an empty state here says that cannot be
     * a string. Given instead of `hint`, never as well: two hints under one
     * headline is the shape this component exists to make impossible.
     */
    children?: Snippet;
    /** Tight, for a state inside something rather than a state that is a view. */
    inline?: boolean;
  }

  let { tone, headline, hint = "", children, inline = false }: Props = $props();
</script>

{#if tone !== "content"}
  <div class="instead" class:inline data-tone={tone} role="status">
    {#if !inline}
      {#if tone === "failed"}
        <!--
          The mark says Sill is fine and the shelf is empty; a failure is not
          that, so it gets its own. Small and in the one colour the theme
          reserves for something being wrong, rather than a red panel: the
          sentence is what has to be read, and a loud frame around it would
          make a refused command look like data loss.
        -->
        <svg
          class="wrong"
          width="32"
          height="32"
          viewBox="0 0 24 24"
          aria-hidden="true"
        >
          <path
            d="M12 3.8 21 19.5H3Z"
            fill="none"
            stroke="currentColor"
            stroke-width="1.6"
            stroke-linejoin="round"
          />
          <path
            d="M12 9.6v4.2"
            stroke="currentColor"
            stroke-width="1.6"
            stroke-linecap="round"
          />
          <circle cx="12" cy="16.6" r="0.9" fill="currentColor" />
        </svg>
      {:else}
        <img src="/sill.png" alt="" width="32" height="32" draggable="false" />
      {/if}
    {/if}

    <span class="headline">{headline}</span>
    {#if children}
      <span class="hint">{@render children()}</span>
    {:else if hint}
      <span class="hint">{hint}</span>
    {/if}
  </div>
{/if}

<style>
  /*
   * Empty, loading and failed states, designed rather than written.
   *
   * A bare centred string is a placeholder, not a finished state. Every one of
   * these gets the mark, a headline and an optional hint, so an empty list
   * looks like a considered answer instead of a view that failed to load.
   */
  .instead {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: var(--space-2);
    flex: 1;
    min-height: 132px;
    padding: var(--space-6) var(--space-4);
    text-align: center;
  }

  /*
   * The same words with the furniture taken away.
   *
   * `flex: initial` rather than nothing: this is dropped into flex columns
   * that would otherwise stretch a two line paragraph down a whole panel.
   *
   * The inset is a settings row's own, which is where the rows this replaces
   * would have started. The four panels that wrote this by hand disagreed
   * about it: two ran the sentence to the card's edge, one indented it by 4px
   * and one by 12, so the same state looked like four states depending on
   * which screen you were on.
   */
  .instead.inline {
    display: block;
    flex: initial;
    min-height: 0;
    padding: var(--space-3) var(--space-4);
    text-align: left;
  }

  img {
    opacity: var(--opacity-faint);
    -webkit-user-drag: none;
  }

  /*
   * The one thing on a failed state that is coloured, and it is the mark.
   *
   * Not faded, unlike the Sill mark above it, because this one is carrying
   * something. The words underneath stay the colour every other headline is:
   * a red glyph and a red sentence is the same fact said twice and turns a
   * refused command into a screen that looks like data loss.
   */
  .wrong {
    color: var(--danger);
  }

  .headline {
    display: block;
    font-size: var(--text-body);
    font-weight: var(--weight-medium);
    color: var(--text-2);
  }


  .hint {
    display: block;
    max-width: 46ch;
    font-size: var(--text-meta);
    /* A ratio rather than the --line-* tokens, which state a row box in px so
       a list keeps its height when the interface face changes. This wraps, so
       what it wants is the space between two lines of prose. */
    line-height: 1.6;
    color: var(--text-3);
  }

  /* Inline states sit in body copy rather than under a mark, so the headline
     is the same size as the text around it and the hint follows it in the
     same paragraph flow. */
  .instead.inline .headline {
    font-size: var(--text-meta);
    font-weight: var(--weight-body);
  }

  .instead.inline .hint {
    max-width: none;
    margin-top: var(--space-half);
  }

  /*
   * Inline has no mark, so the colour moves to the words.
   *
   * Still exactly one coloured thing, which is the rule: the pane density puts
   * it on the glyph because there is one, and this puts it on the headline
   * because there is not. Without it a failure inside a settings card would be
   * a grey sentence where an empty one goes, which is the extension store's
   * old fault reappearing one density down.
   */
  .instead.inline[data-tone="failed"] .headline {
    color: var(--danger);
  }
</style>

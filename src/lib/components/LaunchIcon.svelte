<script lang="ts">
  /**
   * The icon on a row, and the tile it sits in.
   *
   * ## The flash, and why it is gone
   *
   * This asked for its icon in an effect and drew the lettered tile until the
   * answer came back, so every row began as a letter and became a picture.
   * Typing makes new rows on every keystroke and nearly all of them are
   * applications that were on screen a moment ago, which made it a flash per
   * row per keystroke for icons the window already had in hand.
   *
   * Two halves fix it, and neither of them is a fade:
   *
   * 1. **An answer already held is read without waiting.** `knownIcon` is
   *    synchronous, so a path resolved earlier this session paints its icon on
   *    the first frame. This is the case that was happening constantly.
   * 2. **An answer not yet held reserves the tile instead of guessing.** The
   *    slot is drawn empty until the shell answers. A letter shown before the
   *    answer is information the window does not have yet, and replacing it a
   *    frame later is the part that catches the eye.
   *
   * The letter is not gone. It is what a row shows once the answer is known to
   * be "no icon", which is a fact rather than a placeholder, and for a path
   * the shell cannot answer for at all that is known without asking, so those
   * rows are lettered from the first frame as they always were.
   *
   * The other fix the audit offered was to send icons with the search results.
   * Refused: the empty-query payload is 36.9 KB against a 40 KB budget, and a
   * data URI per row is a few kilobytes each, so a screen of results would
   * several times over the budget for a picture most rows already have.
   */
  import { appIcon, hasShellIcon, knownIcon } from "$lib/exthost/commands";

  interface Props {
    /** A real file the shell can read an icon from, when there is one. */
    path: string;
    /** Falls back to the first letter of this. */
    label: string;
    /** False for entries whose icon would say nothing, e.g. a bundled .js. */
    resolvable: boolean;
  }

  let { path, label, resolvable }: Props = $props();

  const askable = $derived(hasShellIcon(path, resolvable));

  /** What is known about this path without waiting for anything. */
  const held = $derived(askable ? knownIcon(path) : { uri: null });

  /** What arrived while this row was on screen, and which path it was for. */
  let arrived = $state<{ path: string; uri: string | null } | null>(null);

  /**
   * The answer, or `undefined` while there is not one.
   *
   * `null` inside it is an answer: this has no icon, draw the letter.
   */
  const answer = $derived(
    held ?? (arrived?.path === path ? { uri: arrived.uri } : undefined),
  );

  $effect(() => {
    if (!askable || held) return;

    let cancelled = false;
    void appIcon(path).then((uri) => {
      if (!cancelled) arrived = { path, uri };
    });

    return () => {
      cancelled = true;
    };
  });

  const initial = $derived((label.trim()[0] ?? "?").toUpperCase());
</script>

<span class="icon" class:lettered={answer !== undefined && !answer.uri}>
  {#if answer?.uri}
    <img src={answer.uri} alt="" />
  {:else if answer}
    <span class="initial">{initial}</span>
  {/if}
</span>

<style>
  /*
   * 26px, which is the icon slot every row uses.
   *
   * This was 22 while `SettingsIcon`, the emoji glyph and the calculator's
   * `=` were all 26, so an application sat visibly smaller than a Sill
   * command directly above it in the same list. The slot is one size.
   *
   * Still well inside the source-resolution rule: `SHDefExtractIconW` is
   * asked for 64px, so 26 is a 2.5x downscale and `icons_arrive_with_more_
   * pixels_than_they_are_drawn_at` (which asserts at least 48) still holds.
   */
  .icon {
    flex: none;
    display: grid;
    place-items: center;
    width: var(--icon-tile);
    height: var(--icon-tile);
    border-radius: var(--radius-sm);
    overflow: hidden;
  }

  /*
   * The letter is a tile; a real icon sits on nothing, because most already
   * carry their own shape and background.
   *
   * Keyed on the letter rather than on the absence of an image, which is the
   * difference between the two. A row waiting for an answer holds the slot open
   * and draws nothing in it, so the icon lands on the ground it will keep. Read
   * the other way round, the tile appeared under every row and then vanished
   * from most of them, which is the flash a second time in a quieter colour.
   */
  .icon.lettered {
    background-color: var(--fill-2);
    box-shadow: var(--bevel-tile);
  }

  img {
    width: 100%;
    height: 100%;
    object-fit: contain;
  }

  .initial {
    font-size: var(--text-meta);
    font-weight: var(--weight-strong);
    color: var(--text-2);
    line-height: 1;
  }
</style>

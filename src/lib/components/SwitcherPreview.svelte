<script lang="ts">
  /**
   * A picture of the window under the cursor, in the switcher.
   *
   * Four browser windows are four rows reading almost the same, and a title
   * cannot tell them apart. The strip keeps its width whether or not there is
   * a picture in it, so arrowing past a window that refuses to be photographed
   * does not shuffle the list sideways.
   *
   * ## Why the state is here and not in the window
   *
   * A picture is a picture of a moment, and it is only ever looked at while
   * the switcher is on screen. Living here means it is created when the
   * switcher opens and let go of when the switcher closes, without the page
   * holding four variables and two effects for a strip it draws in one mode.
   * Leaving is the component being unmounted, and the captures Rust is holding
   * are released from the teardown, which is the same moment.
   */
  import { forgetPreviews, windowPreview } from "$lib/exthost/commands";

  interface Props {
    /**
     * The window to photograph, which is the row under the cursor.
     *
     * Undefined while there is no row, which empties the strip rather than
     * leaving the last window's picture under a different title.
     */
    entrypoint: string | undefined;
  }

  let { entrypoint }: Props = $props();

  let preview = $state<string | null>(null);
  let previewTimer: ReturnType<typeof setTimeout> | undefined;

  /** Which row the picture on screen belongs to, so a stale one is dropped. */
  let previewOf = "";

  /**
   * How long the selection has to settle before a window is photographed.
   *
   * Holding Down walks the list faster than a window can be captured, and
   * every one passed through on the way would be a picture nobody looked at.
   */
  const PREVIEW_SETTLE_MS = 90;

  /**
   * Drops the picture on screen, without telling Rust anything.
   *
   * For opening the switcher on top of itself: whatever was on screen is a
   * picture of a moment that has passed, and the captures behind it are about
   * to be wanted again.
   */
  export function drop() {
    preview = null;
    previewOf = "";
  }

  /**
   * Drops the picture and lets Rust go of what it captured.
   *
   * For the way out that does not change the mode: Escape dismisses the
   * launcher from the switcher without leaving it, so the teardown below never
   * runs and the captures would outlive the window that showed them.
   */
  export function forget() {
    drop();
    void forgetPreviews();
  }

  $effect(() => {
    // Read so this runs again when it changes.
    const wanted = entrypoint;

    clearTimeout(previewTimer);

    if (!wanted) {
      preview = null;
      previewOf = "";
      return;
    }

    if (wanted === previewOf) return;

    previewTimer = setTimeout(() => {
      previewOf = wanted;

      void windowPreview(wanted)
        .then((picture) => {
          // The selection moved on while this was being taken.
          if (previewOf === wanted) preview = picture;
        })
        // A window that closed or refuses to be photographed is not an error
        // worth a message, on the surface or anywhere else. The strip is
        // simply empty, and `windowPreview` answers `null` for the same
        // reasons without failing at all.
        .catch(() => {
          if (previewOf === wanted) preview = null;
        });
    }, PREVIEW_SETTLE_MS);
  });

  /*
   * The captures go when the switcher does.
   *
   * Keeping them would mean showing a window as it was the last time somebody
   * looked rather than as it is, and holding several bitmaps in Rust for a
   * view nobody is on is the opposite of what a launcher at rest should cost.
   */
  $effect(() => () => {
    clearTimeout(previewTimer);
    void forgetPreviews();
  });
</script>

<aside class="preview" aria-hidden="true">
  {#if preview}
    <img src={preview} alt="" />
  {/if}
</aside>

<style>
  /*
   * The strip the picture is drawn in.
   *
   * A fixed width whether or not there is a picture, so arrowing past a window
   * that refuses to be photographed does not shuffle the list sideways. That
   * shuffle is worse than an empty strip: the row under the cursor moves while
   * somebody is reading it.
   */
  .preview {
    flex: none;
    width: 280px;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: var(--space-3);
    overflow: hidden;
  }

  .preview img {
    max-width: 100%;
    max-height: 100%;
    border-radius: var(--radius-sm);
    /* The picture is of somebody's window, which may be any colour and may
       end in a flat edge against the launcher's own. A hairline separates the
       two without drawing a frame around it. */
    box-shadow: var(--ring-outside);
    object-fit: contain;
  }
</style>

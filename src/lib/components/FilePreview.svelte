<script lang="ts">
  /**
   * A look inside the file under the cursor.
   *
   * A path in a subtitle says where something is and not what it is, and two
   * files called `notes.md` in two folders are two rows nobody can tell apart
   * until they open one. A first screenful of the text, or the picture itself,
   * answers in a glance.
   *
   * ## Why it waits, and what it never does
   *
   * Reading somebody's file is not free and it is not private, so it happens
   * for the row that is actually selected and only once the selection has
   * stopped moving. Holding Down walks the list faster than a file can be read
   * and every file passed through on the way would be one Sill opened for
   * nothing.
   *
   * Rust refuses a folder, an executable, an archive, a picture too big to be
   * worth sending, and anything whose bytes are still in somebody's cloud, so
   * an empty strip is the common answer rather than the failure case.
   *
   * ## Why the state is here and not in the page
   *
   * The same reason the switcher's is. A preview is only ever looked at while
   * this is on screen, so it is created when the list draws files and let go of
   * when it stops, and what Rust is holding goes with it. The window being
   * hidden is the other moment: `visible` reports it, because up to twelve
   * previews of somebody's pictures resident behind a hidden launcher is
   * exactly what a launcher at rest must not cost.
   */
  import { filePreview, forgetFilePreviews, type FileLook } from "$lib/exthost/commands";
  import { whenHidden } from "$lib/visible";

  interface Props {
    /**
     * The file to look inside, which is the row under the cursor.
     *
     * Undefined while the selected row is not a file, which empties the strip
     * rather than leaving one file's text under another file's name.
     */
    path: string | undefined;
  }

  let { path }: Props = $props();

  let look = $state<FileLook | null>(null);
  let timer: ReturnType<typeof setTimeout> | undefined;

  /** Which file the text on screen belongs to, so a stale one is dropped. */
  let lookingAt = "";

  /**
   * How long the selection has to settle before a file is opened.
   *
   * The same wait the switcher uses, and for the same reason. It is also the
   * wait that keeps this off the keystroke path entirely: typing moves the
   * selection, and nothing is read until the typing stops.
   */
  const SETTLE_MS = 90;

  $effect(() => {
    // Read so this runs again when it changes.
    const wanted = path;

    clearTimeout(timer);

    if (!wanted) {
      look = null;
      lookingAt = "";
      return;
    }

    if (wanted === lookingAt) return;

    timer = setTimeout(() => {
      lookingAt = wanted;

      void filePreview(wanted)
        .then((found) => {
          // The selection moved on while this was being read.
          if (lookingAt === wanted) look = found;
        })
        // A file that cannot be read is not an error worth a message, on the
        // surface or anywhere else. The strip is simply empty, and
        // `filePreview` answers null for the same reasons without failing.
        .catch(() => {
          if (lookingAt === wanted) look = null;
        });
    }, SETTLE_MS);
  });

  /*
   * Hidden is the same as gone, as far as what is held goes.
   *
   * Escape dismisses the launcher without unmounting anything, so the teardown
   * below never runs and up to twelve previews would sit in Rust behind a
   * window nobody can see.
   */
  $effect(() =>
    whenHidden(() => {
      clearTimeout(timer);
      look = null;
      lookingAt = "";
      void forgetFilePreviews();
    }),
  );

  /* And what is held goes when the list showing it does. */
  $effect(() => () => {
    clearTimeout(timer);
    void forgetFilePreviews();
  });
</script>

<aside class="look" aria-hidden="true">
  {#if look?.kind === "image"}
    <img src={look.body} alt="" />
  {:else if look?.kind === "text"}
    <pre>{look.body}</pre>
  {/if}
</aside>

<style>
  /*
   * The strip the preview is drawn in.
   *
   * A fixed width whether or not there is anything in it, so arrowing past a
   * row with nothing to show does not shuffle the list sideways. That shuffle
   * is worse than an empty strip: the row under the cursor moves while
   * somebody is reading it.
   */
  .look {
    flex: none;
    width: 280px;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: var(--space-3);
    overflow: hidden;
  }

  .look img {
    max-width: 100%;
    max-height: 100%;
    border-radius: var(--radius-sm);
    /* The picture is somebody's own and may end in a flat edge against the
       launcher's. A hairline separates the two without drawing a frame. */
    box-shadow: var(--ring-outside);
    object-fit: contain;
  }

  /*
   * The text is a glance, not a reader.
   *
   * It does not scroll and it does not wrap: a line that runs off the edge
   * still says what the file is, and a wrapped one turns eight lines of code
   * into a paragraph that says less.
   */
  .look pre {
    margin: 0;
    align-self: stretch;
    width: 100%;
    overflow: hidden;
    font-family: var(--font-mono);
    font-size: var(--text-micro);
    line-height: 1.5;
    color: var(--text-3);
    white-space: pre;
  }
</style>

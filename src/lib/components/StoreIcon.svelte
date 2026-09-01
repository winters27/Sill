<script lang="ts">
  /**
   * An extension's icon in the store, or a lettered tile when it has none.
   *
   * The same shape and the same fallback as `LaunchIcon`, which is what keeps
   * a store row looking like a launcher row. It is a separate component
   * because the two answer different questions: `LaunchIcon` asks the shell
   * to extract an icon out of a file on this machine, and this loads a URL
   * from the catalogue. Sharing one component would mean a prop that switches
   * between a Tauri command and an `<img>`, which is two components wearing
   * one name.
   *
   * ## Lazily, and quietly
   *
   * `loading="lazy"` because a browse answers with up to eighty rows and only
   * a handful are on screen: without it, opening the store would be eighty
   * requests to somebody else's asset host in one go. `referrerpolicy` because
   * there is no reason for that host to learn anything about where the request
   * came from.
   *
   * A URL that fails to load falls back to the letter rather than leaving the
   * broken-image glyph, which is the one state that reads as a bug.
   */
  interface Props {
    /** The catalogue's icon URL, or empty. */
    src: string;
    /** Falls back to the first letter of this. */
    label: string;
    size?: number;
  }

  let { src, label, size = 26 }: Props = $props();

  let failed = $state(false);

  /* A new row in the same slot is a new icon, so a previous failure must not
     follow it. Keyed on the URL rather than cleared by the parent. */
  $effect(() => {
    void src;
    failed = false;
  });

  const showing = $derived(src !== "" && !failed);
  const initial = $derived((label.trim()[0] ?? "?").toUpperCase());
</script>

<span
  class="icon"
  class:has-image={showing}
  style:width="{size}px"
  style:height="{size}px"
>
  {#if showing}
    <img
      {src}
      alt=""
      loading="lazy"
      decoding="async"
      referrerpolicy="no-referrer"
      onerror={() => (failed = true)}
    />
  {:else}
    <span class="initial">{initial}</span>
  {/if}
</span>

<style>
  .icon {
    flex: none;
    display: grid;
    place-items: center;
    border-radius: var(--radius-sm);
    overflow: hidden;
  }

  /* The placeholder is a tile; a real icon sits on nothing, because most
     already carry their own shape and background. */
  .icon:not(.has-image) {
    background-color: var(--fill-2);
    box-shadow: var(--bevel-tile);
  }

  img {
    width: 100%;
    height: 100%;
    object-fit: contain;
  }

  .initial {
    color: var(--text-2);
    font-size: var(--text-meta);
    font-weight: var(--weight-strong);
    line-height: 1;
  }
</style>

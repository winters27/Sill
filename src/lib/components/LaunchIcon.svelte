<script lang="ts">
  import { appIcon } from "$lib/exthost/commands";

  interface Props {
    /** A real file the shell can read an icon from, when there is one. */
    path: string;
    /** Falls back to the first letter of this. */
    label: string;
    /** False for entries whose icon would say nothing, e.g. a bundled .js. */
    resolvable: boolean;
  }

  let { path, label, resolvable }: Props = $props();

  let src = $state<string | null>(null);

  // Extension commands have no icon of their own yet, so only applications
  // are worth asking the shell about. A packaged app is identified by an
  // AppUserModelID rather than a file, and the shell cannot make an icon out
  // of that, so those fall through to the lettered tile.
  $effect(() => {
    if (!resolvable || !path || path.startsWith("shell:AppsFolder")) {
      src = null;
      return;
    }

    let cancelled = false;
    void appIcon(path).then((uri) => {
      if (!cancelled) src = uri;
    });

    return () => {
      cancelled = true;
    };
  });

  const initial = $derived((label.trim()[0] ?? "?").toUpperCase());
</script>

<span class="icon" class:has-image={src !== null}>
  {#if src}
    <img {src} alt="" />
  {:else}
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
    width: 26px;
    height: 26px;
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
    font-size: var(--text-meta);
    font-weight: var(--weight-strong);
    color: var(--text-2);
    line-height: 1;
  }
</style>

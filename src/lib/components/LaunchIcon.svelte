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
  .icon {
    flex: none;
    display: grid;
    place-items: center;
    width: 22px;
    height: 22px;
    border-radius: 5px;
    overflow: hidden;
  }

  /* The placeholder is a tile; a real icon sits on nothing, because most
     already carry their own shape and background. */
  .icon:not(.has-image) {
    background-color: rgba(var(--accent-rgb), 0.12);
    box-shadow: var(--bevel-tile);
  }

  img {
    width: 100%;
    height: 100%;
    object-fit: contain;
  }

  .initial {
    font-size: 11px;
    font-weight: 600;
    color: var(--text-muted);
    line-height: 1;
  }
</style>

<script lang="ts">
  /**
   * The icon an extension asked for, in the slot every other row uses.
   *
   * ## Three things, drawn three ways
   *
   * A picture is a picture. A character the extension supplied is printed. A
   * name is a mark, and the set below is Sill's own: one viewBox, one stroke
   * width, one join, so a list of them reads as a family rather than as a
   * dozen drawings that happen to be the same size. That is the same rule
   * `MarkupIcon` follows and it is why they can sit in one list together.
   *
   * ## The names that are not here
   *
   * Raycast publishes around two hundred and fifty icon names and this draws
   * the ones the store actually reaches for. A name with no mark falls back to
   * its own first letter on a tile, which is the launcher's existing answer
   * for an application whose icon the shell will not give up, so an unfamiliar
   * name looks like something Sill drew rather than like something that
   * failed. The remaining names are artwork, and artwork is a separate job.
   *
   * A relative path into an extension's own assets arrives here as a name too,
   * and gets the same tile. The window does not know where an installed
   * extension lives, so the alternative would be a broken image on every row.
   */
  import type { ExtIcon } from "$lib/exthost/present";

  interface Props {
    icon: ExtIcon;
    /** Small enough to sit inside an accessory pill, when it has to. */
    small?: boolean;
  }

  let { icon, small = false }: Props = $props();

  /** The letter a name with no mark falls back to. */
  const initial = $derived(
    icon.kind === "mark" ? ((icon.name.trim()[0] ?? "?").toUpperCase()) : "",
  );

  /**
   * Whether this name has a mark drawn for it below.
   *
   * A list rather than a chain of `{:else if}` with a default at the end. The
   * chain is the shape this project keeps being bitten by, because the case
   * that falls off the end is silent; here the fallback is a decision the
   * markup makes on purpose, and adding a mark means adding its name in two
   * adjacent places rather than remembering to.
   */
  const MARKS = new Set([
    "Star",
    "StarCircle",
    "Circle",
    "Dot",
    "CheckCircle",
    "Checkmark",
    "XMarkCircle",
    "Xmark",
    "Clipboard",
    "CopyClipboard",
    "Document",
    "Folder",
    "Globe",
    "Link",
    "Person",
    "PersonCircle",
    "Calendar",
    "Clock",
    "Gear",
    "Cog",
    "Trash",
    "Plus",
    "Minus",
    "MagnifyingGlass",
    "Pencil",
    "Terminal",
    "Code",
    "Download",
    "Upload",
    "Tag",
    "Bookmark",
    "Envelope",
    "Eye",
    "Heart",
    "House",
    "Key",
    "Lock",
    "Bolt",
    "Info",
    "Warning",
    "ExclamationMark",
    "Window",
    "AppWindow",
    "Text",
    "List",
    "Image",
    "Video",
    "Music",
    "Play",
  ]);

  const drawn = $derived(icon.kind === "mark" && MARKS.has(icon.name) ? icon.name : "");

  /**
   * A picture that would not load, remembered so it is not drawn again.
   *
   * An icon source is a URL somebody else wrote, and the network is somebody
   * else's too. Left alone the browser draws its own torn-page glyph, which is
   * the one thing on a row that says Sill is broken rather than that a host is
   * down. Falling back to the letter tile makes it look like a row without a
   * picture, which is what it is.
   *
   * Keyed by the source, so a re-render with a different one tries again.
   */
  let refused = $state("");

  const broke = $derived(icon.kind === "image" && refused === icon.src);
</script>

<span
  class="ext-icon"
  class:small
  class:lettered={(icon.kind === "mark" && !drawn) || broke}
  style={icon.tint ? `color: ${icon.tint}` : undefined}
>
  {#if icon.kind === "image" && !broke}
    <img src={icon.src} alt="" onerror={() => (refused = icon.src)} />
  {:else if icon.kind === "glyph"}
    <span class="glyph">{icon.text}</span>
  {:else if drawn}
    <svg
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      stroke-width="1.75"
      stroke-linecap="round"
      stroke-linejoin="round"
      aria-hidden="true"
    >
      {#if drawn === "Star" || drawn === "StarCircle"}
        <path d="M12 4.5 14.3 9.4l5.2.7-3.8 3.7.9 5.2-4.6-2.5-4.6 2.5.9-5.2L4.5 10l5.2-.7Z" />
      {:else if drawn === "Circle"}
        <circle cx="12" cy="12" r="7.5" />
      {:else if drawn === "Dot"}
        <circle cx="12" cy="12" r="3.5" fill="currentColor" stroke="none" />
      {:else if drawn === "CheckCircle"}
        <circle cx="12" cy="12" r="7.5" />
        <path d="m8.8 12.2 2.2 2.2 4.2-4.6" />
      {:else if drawn === "Checkmark"}
        <path d="m5.5 12.6 4 4L18.5 7" />
      {:else if drawn === "XMarkCircle"}
        <circle cx="12" cy="12" r="7.5" />
        <path d="m9.4 9.4 5.2 5.2M14.6 9.4l-5.2 5.2" />
      {:else if drawn === "Xmark"}
        <path d="m6.5 6.5 11 11M17.5 6.5l-11 11" />
      {:else if drawn === "Clipboard" || drawn === "CopyClipboard"}
        <rect x="6" y="5" width="12" height="15" rx="2" />
        <path d="M9.5 5V3.8h5V5" />
      {:else if drawn === "Document" || drawn === "Text"}
        <path d="M7 4h6.5L18 8.5V20H7Z" />
        <path d="M13.2 4v4.8H18" />
      {:else if drawn === "Folder"}
        <path d="M4 7.5h5.4l1.8 2H20V19H4Z" />
      {:else if drawn === "Globe"}
        <circle cx="12" cy="12" r="7.5" />
        <path d="M4.6 12h14.8M12 4.6c2 2.2 3 4.7 3 7.4s-1 5.2-3 7.4c-2-2.2-3-4.7-3-7.4s1-5.2 3-7.4Z" />
      {:else if drawn === "Link"}
        <path d="M10 14a3.6 3.6 0 0 1 0-5l2.4-2.4a3.6 3.6 0 0 1 5 5L16 13" />
        <path d="M14 10a3.6 3.6 0 0 1 0 5L11.6 17.4a3.6 3.6 0 0 1-5-5L8 11" />
      {:else if drawn === "Person" || drawn === "PersonCircle"}
        <circle cx="12" cy="9" r="3.2" />
        <path d="M5.8 19.4a6.4 6.4 0 0 1 12.4 0" />
      {:else if drawn === "Calendar"}
        <rect x="4.5" y="6" width="15" height="13.5" rx="2" />
        <path d="M4.5 10.4h15M9 4.4v3.2M15 4.4v3.2" />
      {:else if drawn === "Clock"}
        <circle cx="12" cy="12" r="7.5" />
        <path d="M12 7.8V12l3 1.8" />
      {:else if drawn === "Gear" || drawn === "Cog"}
        <circle cx="12" cy="12" r="2.8" />
        <path
          d="M12 4.2v2M12 17.8v2M19.8 12h-2M6.2 12h-2M17.5 6.5l-1.4 1.4M7.9 16.1l-1.4 1.4M17.5 17.5l-1.4-1.4M7.9 7.9 6.5 6.5"
        />
      {:else if drawn === "Trash"}
        <path d="M5.5 7.5h13M9.5 7.5V5.4h5v2.1M7.2 7.5 8 19.6h8l.8-12.1" />
      {:else if drawn === "Plus"}
        <path d="M12 5.8v12.4M5.8 12h12.4" />
      {:else if drawn === "Minus"}
        <path d="M5.8 12h12.4" />
      {:else if drawn === "MagnifyingGlass"}
        <circle cx="11" cy="11" r="5.6" />
        <path d="m15.2 15.2 4 4" />
      {:else if drawn === "Pencil"}
        <path d="M4.8 19.2 5.5 15 15.9 4.7a1.9 1.9 0 0 1 2.7 2.7L8.2 17.8Z" />
      {:else if drawn === "Terminal" || drawn === "Code"}
        <rect x="4" y="5" width="16" height="14" rx="2" />
        <path d="m8 10 2.4 2.2L8 14.4M12.8 15h3.4" />
      {:else if drawn === "Download"}
        <path d="M12 4.8v9.6M8.2 11l3.8 3.6 3.8-3.6M5.5 18.6h13" />
      {:else if drawn === "Upload"}
        <path d="M12 15.6V6M8.2 9.4 12 5.8l3.8 3.6M5.5 18.6h13" />
      {:else if drawn === "Tag"}
        <path d="M4.6 11.4V5h6.4l8.4 8.4-6.4 6.4Z" />
        <circle cx="8.2" cy="8.4" r="1.1" fill="currentColor" stroke="none" />
      {:else if drawn === "Bookmark"}
        <path d="M6.6 4.6h10.8v15L12 15.8l-5.4 3.8Z" />
      {:else if drawn === "Envelope"}
        <rect x="3.8" y="6" width="16.4" height="12" rx="2" />
        <path d="m4.4 7.4 7.6 5.6 7.6-5.6" />
      {:else if drawn === "Eye"}
        <path d="M2.8 12S6.4 6.4 12 6.4 21.2 12 21.2 12 17.6 17.6 12 17.6 2.8 12 2.8 12Z" />
        <circle cx="12" cy="12" r="2.6" />
      {:else if drawn === "Heart"}
        <path
          d="M12 19.2 5.4 12.8a3.9 3.9 0 0 1 5.5-5.5l1.1 1 1.1-1a3.9 3.9 0 0 1 5.5 5.5Z"
        />
      {:else if drawn === "House"}
        <path d="M4.4 11 12 4.6l7.6 6.4v8.4H4.4Z" />
      {:else if drawn === "Key"}
        <circle cx="8.4" cy="9.6" r="3.6" />
        <path d="m11 12.2 7 7M15.4 16.6l1.8-1.8M17.6 18.8l1.8-1.8" />
      {:else if drawn === "Lock"}
        <rect x="5.5" y="10.4" width="13" height="9.2" rx="2" />
        <path d="M8.6 10.4V8a3.4 3.4 0 0 1 6.8 0v2.4" />
      {:else if drawn === "Bolt"}
        <path d="M13.4 3.6 6.6 13.2h4.6L10.6 20.4l6.8-9.6h-4.6Z" />
      {:else if drawn === "Info"}
        <circle cx="12" cy="12" r="7.5" />
        <path d="M12 11.2v4.6" />
        <circle cx="12" cy="8.4" r="0.9" fill="currentColor" stroke="none" />
      {:else if drawn === "Warning" || drawn === "ExclamationMark"}
        <path d="M12 4.4 20.6 19.4H3.4Z" />
        <path d="M12 10.2v3.6" />
        <circle cx="12" cy="16.4" r="0.9" fill="currentColor" stroke="none" />
      {:else if drawn === "Window" || drawn === "AppWindow"}
        <rect x="3.8" y="5" width="16.4" height="14" rx="2" />
        <path d="M3.8 9.2h16.4" />
      {:else if drawn === "List"}
        <path d="M8.4 7.4h11M8.4 12h11M8.4 16.6h11M4.8 7.4h.02M4.8 12h.02M4.8 16.6h.02" />
      {:else if drawn === "Image"}
        <rect x="3.8" y="5.4" width="16.4" height="13.2" rx="2" />
        <path d="m4.6 16 4.2-4.2 3.4 3.4 2.6-2.6 4.4 4.4" />
        <circle cx="9" cy="9.4" r="1.3" />
      {:else if drawn === "Video"}
        <rect x="3.4" y="6.6" width="12.4" height="10.8" rx="2" />
        <path d="m16.2 12 4.4-2.8v5.6Z" />
      {:else if drawn === "Music"}
        <path d="M9.4 17.4V6.4l8-1.6v11" />
        <circle cx="7.2" cy="17.4" r="2.2" />
        <circle cx="15.2" cy="15.8" r="2.2" />
      {:else if drawn === "Play"}
        <path d="M8.4 5.6 18.6 12 8.4 18.4Z" />
      {/if}
    </svg>
  {:else}
    <!-- Empty when a picture refused to load: there is no name to take a
         letter from, and the tile alone is the launcher's own way of saying a
         row has no icon rather than that something went wrong. -->
    <span class="initial">{initial}</span>
  {/if}
</span>

<style>
  /*
   * The slot every row's icon sits in, which is the launcher's own.
   *
   * An extension's list and the root list are the same list to look down, so
   * an extension's icon is the size an application's icon is. The small
   * variant is the one that goes inside an accessory pill, where the text
   * beside it sets the height.
   */
  .ext-icon {
    flex: none;
    display: grid;
    place-items: center;
    width: var(--icon-tile);
    height: var(--icon-tile);
    border-radius: var(--radius-sm);
    overflow: hidden;
    color: var(--text-2);
  }

  .ext-icon.small {
    width: var(--icon-tile-xs);
    height: var(--icon-tile-xs);
  }

  /*
   * The tile is for the letter only, exactly as it is on an application row.
   * A real picture and a drawn mark both sit on nothing, because giving a mark
   * a background makes it a button and giving a picture one puts a box behind
   * an icon that already has its own shape.
   */
  .ext-icon.lettered {
    background-color: var(--fill-2);
    box-shadow: var(--bevel-tile);
  }

  img {
    width: 100%;
    height: 100%;
    object-fit: contain;
  }

  svg {
    width: 100%;
    height: 100%;
  }

  .glyph {
    font-size: var(--glyph-sm);
    line-height: 1;
  }

  .ext-icon.small .glyph {
    font-size: var(--text-micro);
  }

  .initial {
    font-size: var(--text-meta);
    font-weight: var(--weight-strong);
    color: var(--text-2);
    line-height: 1;
  }
</style>

<script module lang="ts">
  export type IconName =
    | "general"
    | "appearance"
    | "dictation"
    | "snippets"
    | "emoji"
    | "shortcuts"
    | "quicklinks"
    | "clipboard"
    | "history"
    | "sources"
    | "files"
    | "extensions"
    | "advanced"
    | "about";
</script>

<script lang="ts">
  /**
   * The settings icon set.
   *
   * Two kinds, and which one is used depends only on whether art exists for
   * that panel. Most panels have a drawn icon in `static/settings`, a
   * self-contained coloured plaque. Anything without art falls back to the
   * line glyph below, which inherits `currentColor` and so follows the theme
   * with no wiring.
   *
   * The fallback is not dead weight: it is what stops a panel added before
   * its art is drawn from showing a broken image in the sidebar.
   *
   * `size` is the **tile**, not the glyph, and the component owns the tile
   * rather than its callers. A line glyph needs a tinted tile behind it to
   * read as an icon; a coloured plaque already is one, and put inside that
   * same tile it becomes a box in a box. Only this component knows which it
   * drew, so only this component can decide.
   */
  interface Props {
    name: IconName;
    /** The tile's size. The glyph inside a fallback tile is scaled from it. */
    size?: number;
  }

  let { name, size = 26 }: Props = $props();

  /** Panels with drawn art. The rest use the line glyphs below. */
  const ART = new Set<IconName>([
    "general",
    "appearance",
    "dictation",
    "clipboard",
    "sources",
    "files",
    "extensions",
    "advanced",
    "about",
  ]);

  const drawn = $derived(ART.has(name));

  /**
   * The widths on disk, offered to the browser so it can pick the one it will
   * actually draw.
   *
   * Without this the browser takes one large file and scales it down itself,
   * which for a 26px icon is a 4.9x reduction through a cheap filter, and the
   * fine work inside each glyph turns to mush.
   *
   * These are the exact sizes drawn, 26 and 38 at 1x, 2x and 3x, rather than
   * round numbers near them. With `sizes` set to the drawn width the browser
   * resolves against the display's pixel ratio and lands on a straight copy,
   * so on a plain screen nothing is resampled at all.
   */
  const WIDTHS = [26, 38, 52, 76, 78, 114];

  const srcset = $derived(
    WIDTHS.map((w) => `/settings/${name}-${w}.png ${w}w`).join(", "),
  );
</script>

{#if drawn}
  <img
    class="art"
    src="/settings/{name}-52.png"
    {srcset}
    sizes="{size}px"
    alt=""
    width={size}
    height={size}
    draggable="false"
  />
{:else}
  <span class="tile" style="--tile: {size}px" aria-hidden="true">

<svg
    width={Math.round(size * 0.54)}
    height={Math.round(size * 0.54)}
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width="1.7"
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"
>
  {#if name === "general"}
    <!-- Sliders: the general shape of "the knobs". -->
    <path d="M4 6h10M18 6h2M4 12h2M10 12h10M4 18h8M16 18h4" />
    <circle cx="16" cy="6" r="2" />
    <circle cx="8" cy="12" r="2" />
    <circle cx="14" cy="18" r="2" />
  {:else if name === "appearance"}
    <circle cx="12" cy="12" r="9" />
    <path d="M12 3a9 9 0 0 0 0 18 4.5 4.5 0 0 0 0-9 4.5 4.5 0 0 1 0-9Z" />
  {:else if name === "dictation"}
    <!-- A microphone: the one glyph nobody has to be taught. -->
    <rect x="9" y="2.5" width="6" height="11" rx="3" />
    <path d="M5 11a7 7 0 0 0 14 0" />
    <path d="M12 18v3M8.5 21h7" />
  {:else if name === "snippets"}
    <!-- Scissors: the one glyph that says "a saved piece of text". -->
    <circle cx="6" cy="6.5" r="2.6" />
    <circle cx="6" cy="17.5" r="2.6" />
    <path d="M8.3 8 20 17M20 7 8.3 16" />
  {:else if name === "emoji"}
    <!-- A face, because that is what the set is mostly for and it is the one
         glyph nobody has to be taught. -->
    <circle cx="12" cy="12" r="9" />
    <path d="M8.5 10h.01M15.5 10h.01" />
    <path d="M8.5 14.5a4.5 4.5 0 0 0 7 0" />
  {:else if name === "shortcuts"}
    <!-- A keycap. These settings are keys, and a key is what a key looks
         like; a lightning bolt would be saying "fast" about a binding. -->
    <rect x="3" y="6" width="18" height="12" rx="2.5" />
    <path d="M8 11.5h2M14 11.5h2M8.5 14.5h7" />
  {:else if name === "quicklinks"}
    <!-- A chain link: the one glyph that reads as a saved address. -->
    <path d="M10 13.5a4 4 0 0 0 5.7.4l2.6-2.6a4 4 0 0 0-5.7-5.7l-1.5 1.5" />
    <path d="M14 10.5a4 4 0 0 0-5.7-.4l-2.6 2.6a4 4 0 0 0 5.7 5.7l1.5-1.5" />
  {:else if name === "clipboard"}
    <!-- A clipboard, which is the one glyph nobody has to be taught. -->
    <rect x="4.5" y="4" width="15" height="17" rx="2" />
    <path d="M9 4V3a1 1 0 0 1 1-1h4a1 1 0 0 1 1 1v1Z" />
    <path d="M8.5 11h7M8.5 15h4" />
  {:else if name === "history"}
    <!-- A clock with an arrow back round it: the universal "past" glyph. -->
    <path d="M3.5 12a8.5 8.5 0 1 0 2.6-6.1" />
    <path d="M3 4.5V9h4.5" />
    <path d="M12 7.5V12l3 1.8" />
  {:else if name === "sources"}
    <!-- Stacked layers, one per place Sill looks. -->
    <path d="M12 3 3 7.5 12 12l9-4.5L12 3Z" />
    <path d="m3 12 9 4.5L21 12" />
    <path d="m3 16.5 9 4.5 9-4.5" />
  {:else if name === "files"}
    <path d="M4 6.5A1.5 1.5 0 0 1 5.5 5h3.2a1.5 1.5 0 0 1 1.2.6l.9 1.2a1.5 1.5 0 0 0 1.2.6h5.5A1.5 1.5 0 0 1 19 8.9" />
    <path d="M4 6.5v11A1.5 1.5 0 0 0 5.5 19h9.2" />
    <circle cx="17.5" cy="16.5" r="3.5" />
    <path d="m21 20-1.1-1.1" />
  {:else if name === "extensions"}
    <!-- Puzzle piece: the shape every extension gallery uses. -->
    <path
      d="M9 4.5a2 2 0 1 1 4 0V6h3.2a.8.8 0 0 1 .8.8V10h1.5a2 2 0 1 1 0 4H17v3.2a.8.8 0 0 1-.8.8H13v-1.5a2 2 0 1 0-4 0V18H5.8a.8.8 0 0 1-.8-.8V6.8a.8.8 0 0 1 .8-.8H9V4.5Z"
    />
  {:else if name === "advanced"}
    <path d="m6 8 4 4-4 4" />
    <path d="M13 16h5" />
    <rect x="2.5" y="4" width="19" height="16" rx="2.5" />
  {:else}
    <circle cx="12" cy="12" r="9" />
    <path d="M12 11v5.5M12 7.8h.01" />
  {/if}
</svg>
  </span>
{/if}

<style>
  /* No border-radius: the corner is already cut into the art's alpha, and
     clipping it a second time lands a hard edge just inside an antialiased
     one. */
  .art {
    display: block;
    flex: none;
    -webkit-user-drag: none;
  }

  /* Only ever behind a line glyph. See the note on `size` above. */
  .tile {
    display: grid;
    place-items: center;
    width: var(--tile);
    height: var(--tile);
    flex: none;
    border-radius: 22%;
    background: rgba(var(--accent-rgb), 0.1);
    box-shadow: var(--bevel-tile);
    color: var(--core-foreground);
  }
</style>

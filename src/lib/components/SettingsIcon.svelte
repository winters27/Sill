<script module lang="ts">
  /**
   * Every name this can draw, with the type derived from it.
   *
   * A list rather than a bare union because the design gallery draws all of
   * them, and a union it cannot read means a second copy of the names, which
   * is a gallery that silently stops being the whole set.
   */
  export const PANEL_ICONS = [
    "general",
    "appearance",
    "ai",
    "dictation",
    "tts",
    "widgets",
    "snippets",
    "emoji",
    "shortcuts",
    "quicklinks",
    "automations",
    "mcp",
    "clipboard",
    "history",
    "sources",
    "files",
    "browsers",
    "websearch",
    "screenshot",
    "extensions",
    "scripts",
    "advanced",
    "about",
  ] as const;

  export type IconName = (typeof PANEL_ICONS)[number];
</script>

<script lang="ts">
  /**
   * The settings icon set. Artwork where a panel has some, an etched hairline
   * drawing where it does not.
   *
   * ## Two kinds, and this is not a transition
   *
   * All twenty panels have art. The three names that are not panels
   * (`history`, `browsers`, `websearch`) are drawn here instead, so both
   * paths are permanently live and neither is a placeholder waiting to be
   * replaced. Which one is used depends only on whether art exists.
   *
   * The set before this one was seventeen coloured plaques, and it went for
   * reasons worth not repeating: it did not cover every panel, two of its
   * names (`browsers` and `websearch`) were swapped, `scripts` was listed as
   * having art that had never been drawn, and the drawings were bright enough
   * to be the loudest thing in a window whose whole style is restraint.
   *
   * ## How the etch works
   *
   * The fallback glyph is drawn twice from one definition. Once in `--etch`
   * lifted a pixel, which is the shadow on the groove's far wall, and once in
   * `currentColor` at the true position, which is the near lip catching the
   * light. The light comes from above, the same direction `--bevel-tile`
   * lights everything else in the window.
   *
   * `currentColor` is the point of the second pass: the glyph is the same
   * colour as the label beside it and brightens with it. Artwork cannot do
   * that, which is the one thing the drawn path gives up.
   */
  interface Props {
    name: IconName;
    /** The drawing's box. The glyph fills it; there is no tile behind it. */
    size?: number;
  }

  let { name, size = 26 }: Props = $props();

  /**
   * One user unit, in CSS pixels, so the drawing can be specified in pixels.
   *
   * The viewBox is a fixed 24 and the box is not, so a stroke written as a
   * constant would be 0.7px in the settings search and 2.1px in the panel
   * header: the same set arriving as a hairline in one place and a medium
   * weight in another. Dividing through by the box holds the *apparent*
   * weight at 1.35px everywhere, which is what makes a hairline set read as
   * one set.
   */
  const unit = $derived(24 / size);

  /**
   * Below this the groove is thinner than the screen can draw.
   *
   * The offset is one CSS pixel, so at 13px it is half a device pixel on a
   * plain display: not a shadow, just the line rendered twice slightly out of
   * register, which is the definition of blurred. Small sizes get the single
   * clean stroke instead, and lose nothing a person could have seen.
   */
  const etched = $derived(size >= 20);

  /**
   * Panels with drawn artwork. Everything else uses the etched glyph above.
   *
   * The two kinds are not a transition: the twenty panels have art and the
   * three names that are not panels (`history`, `browsers`, `websearch`) do
   * not, so both paths are live and neither is waiting to be finished.
   *
   * A name in here with nothing on disk is worse than a name left out. It
   * takes the `<img>` path, 404s, and skips the fallback that exists to stop
   * exactly that, which is how `scripts` came to draw a broken image in the
   * launcher as well as here. `verify:source` now holds this set against the
   * files, in both directions.
   */
  const ART = new Set<IconName>([
    "general",
    "appearance",
    "ai",
    "dictation",
    "tts",
    "widgets",
    "snippets",
    "emoji",
    "shortcuts",
    "quicklinks",
    "automations",
    "mcp",
    "clipboard",
    "sources",
    "files",
    "screenshot",
    "extensions",
    "scripts",
    "advanced",
    "about",
  ]);

  const drawn = $derived(ART.has(name));

  /**
   * The widths on disk, offered so the browser picks the one it will draw.
   *
   * Without this it takes one large file and scales it down itself, which for
   * a 26px icon is a 4.4x reduction through a cheap filter, and the fine work
   * inside each drawing turns to mush. These are the exact sizes generated,
   * 26 and 38 at 1x, 2x and 3x, so with `sizes` set to the drawn width the
   * browser resolves against the pixel ratio and lands on a straight copy.
   */
  const WIDTHS = [26, 38, 52, 76, 78, 114];

  const srcset = $derived(WIDTHS.map((w) => `/settings/${name}-${w}.png ${w}w`).join(", "));
</script>

{#snippet glyph()}
  {#if name === "general"}
    <!-- Two faders. Vertical rather than the horizontal three-slider
         arrangement, which is the one drawing every icon set already has. -->
    <path d="M8 3.4v3.3M8 11.7v8.9M16 3.4v9.8M16 18.2v2.4" />
    <rect x="4.65" y="6.75" width="6.7" height="4.9" rx="1.1" />
    <rect x="12.65" y="13.25" width="6.7" height="4.9" rx="1.1" />
  {:else if name === "appearance"}
    <!-- Contrast: a circle split down the middle, one half ruled. Hatching
         rather than a filled half, so the set stays stroke-only and the panel
         about how light the window is does not arrive as a solid black mass. -->
    <circle cx="12" cy="12" r="8.35" />
    <path d="M12 3.65v16.7" />
    <path d="M14.3 6.2h3.1M15.5 9.15h3.9M15.9 12.1h4.2M15.5 15.05h3.9M14.3 18h3.1" />
  {:else if name === "ai"}
    <!-- A speech bubble with a spark in it: something answering, rather than
         a robot, which would say the wrong thing about who is talking. -->
    <path
      d="M20.9 8.1v5.2a3.8 3.8 0 0 1-3.8 3.8h-4.6L7.7 21.2v-4.1h-.8a3.8 3.8 0 0 1-3.8-3.8V8.1a3.8 3.8 0 0 1 3.8-3.8h10.2a3.8 3.8 0 0 1 3.8 3.8Z"
    />
    <path d="M12 7.4l1.1 2.2 2.2 1.1-2.2 1.1-1.1 2.2-1.1-2.2-2.2-1.1 2.2-1.1Z" />
  {:else if name === "dictation"}
    <!-- A microphone: the one glyph nobody has to be taught. -->
    <rect x="9.35" y="2.55" width="5.3" height="10.9" rx="2.65" />
    <path d="M5.9 10.6a6.1 6.1 0 0 0 12.2 0" />
    <path d="M12 16.7v4.2M8.6 20.9h6.8" />
  {:else if name === "tts"}
    <!-- A cone and two arcs. Deliberately not a second microphone: dictation
         is sound coming in and this is sound going out, and at 26px the two
         are told apart by the silhouette or not at all. -->
    <path d="M4.2 9.5h3.2l4.6-4v13l-4.6-4H4.2Z" />
    <path d="M15.2 9.3a4 4 0 0 1 0 5.4" />
    <path d="M17.9 6.7a7.7 7.7 0 0 1 0 10.6" />
  {:else if name === "snippets"}
    <!-- Scissors: the one glyph that says "a saved piece of text". -->
    <circle cx="6" cy="6.4" r="2.5" />
    <circle cx="6" cy="17.6" r="2.5" />
    <path d="M8.2 7.9 19.6 17M19.6 7 8.2 16.1" />
  {:else if name === "quicklinks"}
    <!-- A chain link: the one glyph that reads as a saved address. -->
    <path d="M10 13.6a4.1 4.1 0 0 0 5.8.4l2.6-2.6a4.1 4.1 0 0 0-5.8-5.8l-1.5 1.5" />
    <path d="M14 10.4a4.1 4.1 0 0 0-5.8-.4l-2.6 2.6a4.1 4.1 0 0 0 5.8 5.8l1.5-1.5" />
  {:else if name === "automations"}
    <!-- A cycle with both arrowheads: it comes round again on its own. Not a
         clock, which is `history`, and not a lightning bolt, which would be
         saying "fast" about something whose whole point is that it waits. -->
    <path d="M4.4 12a7.6 7.6 0 0 1 7.6-7.6h5" />
    <path d="m14.2 1.7 3.4 2.7-3.4 2.7" />
    <path d="M19.6 12a7.6 7.6 0 0 1-7.6 7.6H7" />
    <path d="m9.8 22.3-3.4-2.7 3.4-2.7" />
  {:else if name === "mcp"}
    <!-- A plug going into a socket: something of somebody else's, connected
         to Sill on purpose. A robot or a brain would be saying "AI", and
         these are ordinary programs on a pipe. -->
    <path d="M9 2.6v4.2M15 2.6v4.2" />
    <rect x="6.4" y="6.8" width="11.2" height="5.6" rx="1.4" />
    <path d="M12 12.4v4.4" />
    <path d="M8.4 16.8h7.2a1 1 0 0 1 1 1v3.6H7.4v-3.6a1 1 0 0 1 1-1Z" />
  {:else if name === "clipboard"}
    <!-- A clipboard, which is the one glyph nobody has to be taught. -->
    <path
      d="M9.2 4H6.6a2.2 2.2 0 0 0-2.2 2.2v12.2a2.2 2.2 0 0 0 2.2 2.2h10.8a2.2 2.2 0 0 0 2.2-2.2V6.2A2.2 2.2 0 0 0 17.4 4h-2.6"
    />
    <rect x="8.75" y="2.15" width="6.5" height="3.9" rx="1.3" />
    <path d="M8.2 11.45h7.6M8.2 15.25h4.8" />
  {:else if name === "emoji"}
    <!-- A face, because that is what the set is mostly for and it is the one
         glyph nobody has to be taught. -->
    <circle cx="12" cy="12" r="8.5" />
    <path d="M8.9 9.5v1.5M15.1 9.5v1.5" />
    <path d="M8.4 14.4a4.4 4.4 0 0 0 7.2 0" />
  {:else if name === "shortcuts"}
    <!-- A keyboard. A lightning bolt would be saying "fast" about a binding,
         and these settings are keys.

         Three marks on the top row rather than two: two of anything sitting
         either side of a bar inside a rounded rectangle reads as a face, and
         this one did. -->
    <rect x="3.2" y="6.2" width="17.6" height="11.6" rx="2.2" />
    <path d="M6.9 11.2h1.9M11.05 11.2h1.9M15.2 11.2h1.9M8 14.8h8" />
  {:else if name === "history"}
    <!-- A clock with an arrow back round it: the universal "past" glyph. -->
    <path d="M3.6 12A8.4 8.4 0 1 0 6.2 5.9" />
    <path d="M3.1 4.4v4.6h4.6" />
    <path d="M12 7.4V12l3 1.8" />
  {:else if name === "sources"}
    <!-- Stacked layers, one per place Sill looks. -->
    <path d="M12 3.2 3.4 7.4 12 11.6l8.6-4.2Z" />
    <path d="m3.4 12 8.6 4.2 8.6-4.2" />
    <path d="m3.4 16.6 8.6 4.2 8.6-4.2" />
  {:else if name === "files"}
    <path
      d="M4 8.6V6.4a1.8 1.8 0 0 1 1.8-1.8h3.4a1.8 1.8 0 0 1 1.4.7l1 1.3a1.8 1.8 0 0 0 1.4.7h5.2A1.8 1.8 0 0 1 20 9.1"
    />
    <path d="M4 8.6v9.2a1.8 1.8 0 0 0 1.8 1.8h8.4" />
    <circle cx="17.4" cy="16.4" r="3.4" />
    <path d="m20.6 19.6 1.4 1.4" />
  {:else if name === "extensions"}
    <!-- Puzzle piece: the shape every extension gallery uses. -->
    <path
      d="M9.2 4.6a2.1 2.1 0 1 1 4.2 0v1.5h3.3a.9.9 0 0 1 .9.9v3.3h1.4a2.1 2.1 0 1 1 0 4.2h-1.4v3.3a.9.9 0 0 1-.9.9h-3.3v-1.5a2.1 2.1 0 1 0-4.2 0v1.5H5.9a.9.9 0 0 1-.9-.9V7a.9.9 0 0 1 .9-.9h3.3Z"
    />
  {:else if name === "scripts"}
    <!-- A page with a prompt on it. The prompt alone is `advanced`, which is
         a terminal; this is the file you keep in a folder, and the two panels
         sit four rows apart in the same sidebar. -->
    <path
      d="M13.2 2.6H6.4a1.8 1.8 0 0 0-1.8 1.8v15.2a1.8 1.8 0 0 0 1.8 1.8h11.2a1.8 1.8 0 0 0 1.8-1.8V8.4Z"
    />
    <path d="M13.2 2.6v5.8H19" />
    <path d="m8 13.2 2 2-2 2" />
    <path d="M12 17.2h3.4" />
  {:else if name === "screenshot"}
    <!-- A frame with the middle marked, which is what picking an area looks
         like before anything has been picked. -->
    <path d="M4 9.2V6a2 2 0 0 1 2-2h3.2" />
    <path d="M14.8 4H18a2 2 0 0 1 2 2v3.2" />
    <path d="M20 14.8V18a2 2 0 0 1-2 2h-3.2" />
    <path d="M9.2 20H6a2 2 0 0 1-2-2v-3.2" />
    <rect x="9.4" y="9.4" width="5.2" height="5.2" rx="1.2" />
  {:else if name === "websearch"}
    <!-- A globe under a lens: the world, and looking something up in it. The
         plain globe is `browsers`; the old artwork had these two the wrong way
         round, which is worth not reproducing. -->
    <circle cx="10.8" cy="10.8" r="6.6" />
    <path d="M4.2 10.8h13.2" />
    <path d="M10.8 4.2a10.4 10.4 0 0 1 0 13.2a10.4 10.4 0 0 1 0-13.2" />
    <path d="m16 16 4 4" />
  {:else if name === "browsers"}
    <!-- A globe: the meridian and the equator are what read as one at this size. -->
    <circle cx="12" cy="12" r="8.5" />
    <path d="M3.5 12h17" />
    <path d="M12 3.5a13 13 0 0 1 0 17a13 13 0 0 1 0-17" />
  {:else if name === "widgets"}
    <!-- Four panes with the clock in one of them. A plain 2x2 grid is the
         layout icon; the odd cell is what says these are assorted small
         things that ride along rather than a way of arranging the window. -->
    <rect x="3.4" y="3.4" width="7.6" height="7.6" rx="1.8" />
    <circle cx="16.8" cy="7.2" r="3.8" />
    <path d="M16.8 5.3v2l1.3.8" />
    <rect x="3.4" y="13" width="7.6" height="7.6" rx="1.8" />
    <rect x="13" y="13" width="7.6" height="7.6" rx="1.8" />
  {:else if name === "advanced"}
    <!-- A terminal window. The page with the same prompt on it is `scripts`. -->
    <rect x="2.6" y="4.4" width="18.8" height="15.2" rx="2.4" />
    <path d="m6.6 10 2.6 2.6-2.6 2.6" />
    <path d="M12.2 15.2h5.2" />
  {:else if name === "about"}
    <!-- The one panel a generic mark is the right answer for. -->
    <circle cx="12" cy="12" r="8.6" />
    <path d="M12 11v5.4M12 7.5v1.1" />
  {:else}
    <!--
      A name with no drawing, which is a panel added before its glyph was.

      An empty frame rather than the question mark this used to be: `about`
      wears the circled mark legitimately, and two identical icons meaning
      "information" and "somebody forgot" is the confusion this whole file is
      here to avoid.
    -->
    <rect x="4.2" y="4.2" width="15.6" height="15.6" rx="3.4" />
    <path d="M12 11.4v1.2" />
  {/if}
{/snippet}

{#if drawn}
  <!-- No border-radius: the corner is cut into the art's own alpha, and
       clipping it a second time lands a hard edge just inside an
       antialiased one. -->
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
  <svg
    width={size}
    height={size}
    viewBox="0 0 24 24"
    fill="none"
    stroke-width={1.35 * unit}
    stroke-linecap="butt"
    stroke-linejoin="miter"
    aria-hidden="true"
  >
    {#if etched}
      <g class="groove" transform="translate(0 {-unit})" stroke-width={1.5 * unit}>
        {@render glyph()}
      </g>
    {/if}
    <g class="edge">{@render glyph()}</g>
  </svg>
{/if}

<style>
  svg,
  .art {
    display: block;
    flex: none;
  }

  .art {
    -webkit-user-drag: none;
  }

  /* The far wall of the groove. Fixed, because it is a shadow: it does not
     brighten when the row it is in does. */
  .groove {
    stroke: var(--etch);
  }

  /* The lit lip, and the icon as far as anybody looking at it is concerned.
     `currentColor` so it is the same colour as the label beside it and moves
     with it from --text-2 to --text-1. */
  .edge {
    stroke: currentColor;
  }
</style>

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
    /*
     * Row marks: kinds of launcher row that have no file to take an icon from
     * and no settings panel to inherit one. Rust names one with
     * `icon: "mark:<name>"` and the row draws it from here, so the gallery
     * shows it and the guards that hold panels to glyphs hold these too.
     * Hairline glyphs until art is drawn for them; see `ART`.
     */
    "arrangements",
    "notes",
    "reminders",
    "fonts",
    "displays",
    "terminals",
    "media",
    "text",
    "picture",
    "confetti",
    "reindex",
    "undo",
    "colour",
    "crop",
    "screen",
    "markup",
    "qr",
    "key",
    "server",
    "clock",
    "weather",
    "transfer",
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
   * All twenty panels have art. The names that are not panels, three
   * sources (`history`, `browsers`, `websearch`) and nine row marks, are
   * drawn here instead, so both paths are permanently live and neither is
   * a placeholder waiting to be replaced. Which one is used depends only on
   * whether art exists.
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
   * names that are not panels (three sources and nine row marks) do not, so
   * both paths are live and neither is waiting to be finished. When art for
   * a row mark lands, its name joins this set and nothing else changes.
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
    "websearch",
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
  {:else if name === "arrangements"}
    <!-- Three windows tiled: a tall one and two stacked, which is what a
         saved arrangement is a picture of. -->
    <rect x="3.5" y="3.5" width="7.4" height="17" rx="1.3" />
    <rect x="13.1" y="3.5" width="7.4" height="7.4" rx="1.3" />
    <rect x="13.1" y="13.1" width="7.4" height="7.4" rx="1.3" />
  {:else if name === "notes"}
    <!-- A page with a turned corner and two lines on it. Not `snippets`,
         which is scissors: a note is a page, a snippet is a cutting. -->
    <path d="M6 3.5h8.4l4.1 4.1v12.9H6Z" />
    <path d="M14.4 3.5v4.1h4.1" />
    <path d="M8.7 12.3h6.6M8.7 15.8h6.6" />
  {:else if name === "reminders"}
    <!-- A bell. The one glyph that says "later, and it will tell you". -->
    <path d="M6.2 16.6c1.3-1.4 1.8-3 1.8-5.1V10a4 4 0 0 1 8 0v1.5c0 2.1.5 3.7 1.8 5.1Z" />
    <path d="M10.3 19.3a1.7 1.7 0 0 0 3.4 0" />
    <path d="M12 3.6v1.6" />
  {:else if name === "fonts"}
    <!-- A capital A over its baseline: the letter is what a font row is a
         sample of, and the rule under it is what makes it type rather than a
         tent. -->
    <path d="M6.2 17.4 11.1 5.2h1.8l4.9 12.2" />
    <path d="M8.3 13.2h7.4" />
    <path d="M4 20.5h16" />
  {:else if name === "displays"}
    <!-- A monitor on its stand. -->
    <rect x="3.5" y="4.5" width="17" height="11.5" rx="1.6" />
    <path d="M12 16v3.5M8.5 19.5h7" />
  {:else if name === "terminals"}
    <!-- A prompt and a cursor block with no window around them, because
         `scripts` is a page with a prompt and `advanced` is a window with one,
         and the three sit near each other in the launcher. -->
    <path d="m5 7.5 5.2 4.5L5 16.5" />
    <rect x="12.5" y="15" width="6.5" height="1.6" rx="0.8" />
  {:else if name === "media"}
    <!-- A play mark in a rounded square: what is playing, as a control. -->
    <rect x="3.5" y="3.5" width="17" height="17" rx="4" />
    <path d="M10 8.4v7.2l6-3.6Z" />
  {:else if name === "text"}
    <!-- Three lines of text with the middle one selected: it is the
         highlighted paragraph itself, so the mark is a selection. -->
    <path d="M5 6.5h14M5 17.5h9" />
    <rect x="4" y="9.6" width="16" height="4.8" rx="1" />
    <path d="M6.5 12h11" />
  {:else if name === "picture"}
    <!-- A framed picture: sun and hills. The last thing copied, when it was
         a picture rather than words. -->
    <rect x="3.5" y="4.5" width="17" height="15" rx="1.8" />
    <circle cx="9" cy="9.3" r="1.6" />
    <path d="m3.8 17.4 5.2-5.1 3.4 3.3 3-2.9 4.8 4.7" />
  {:else if name === "confetti"}
    <!--
      Three pieces of paper climbing a diagonal, with two sequins off it.

      Each piece is a quadrilateral rather than a `rect` with a rotation,
      because every other drawing in this file states its geometry and one
      that carries a transform reads as a different kind of thing. The
      diagonal is the throw: the same two fountains `$lib/confetti` draws,
      reduced to the one gesture that still says it at 26px.
    -->
    <path d="M8.38 18.32 6.52 14.33 3.62 15.68 5.48 19.67Z" />
    <path d="M13.25 13.62 14.76 9.48 11.75 8.39 10.24 12.52Z" />
    <path d="M20.73 6.46 18.32 3.02 15.87 4.74 18.28 8.18Z" />
    <circle cx="4.6" cy="10.6" r="1.05" />
    <circle cx="17.4" cy="16.8" r="1.05" />
  {:else if name === "qr"}
    <!-- Three finder squares and a scattering of modules. Drawn rather
         than borrowed: Phosphor has no QR mark, and `squares-four` reads
         as a grid of apps rather than as something to point a camera at. -->
    <rect x="3.4" y="3.4" width="7" height="7" rx="1" />
    <rect x="13.6" y="3.4" width="7" height="7" rx="1" />
    <rect x="3.4" y="13.6" width="7" height="7" rx="1" />
    <path d="M6.2 6.2h1.4v1.4H6.2zM16.4 6.2h1.4v1.4h-1.4zM6.2 16.4h1.4v1.4H6.2z" />
    <path d="M13.6 13.6h2.6M19 13.6h1.6M13.6 17.2v3.4M17.4 16.4h3.2M17.4 20.6h3.2" />
  {:else if name === "reindex"}
    <!--
      Phosphor arrows-clockwise, the same regular weight `marks.ts` carries.

      Scaled rather than redrawn: Phosphor states its outlines on a 256
      box and every drawing above is stated on a 24 one, and 24/256 is
      the whole of the difference. Filled and unstroked inside the
      group, because a regular-weight outline is a filled shape; its
      optical weight is what `marks.ts` says lets the two sets sit in
      one list.
    -->
    <g transform="scale(0.09375)" fill="currentColor" stroke="none">
      <path d="M224,48V96a8,8,0,0,1-8,8H168a8,8,0,0,1,0-16h28.69L182.06,73.37a79.56,79.56,0,0,0-56.13-23.43h-.45A79.52,79.52,0,0,0,69.59,72.71,8,8,0,0,1,58.41,61.27a96,96,0,0,1,135,.79L208,76.69V48a8,8,0,0,1,16,0ZM186.41,183.29a80,80,0,0,1-112.47-.66L59.31,168H88a8,8,0,0,0,0-16H40a8,8,0,0,0-8,8v48a8,8,0,0,0,16,0V179.31l14.63,14.63A95.43,95.43,0,0,0,130,222.06h.53a95.36,95.36,0,0,0,67.07-27.33,8,8,0,0,0-11.18-11.44Z" />
    </g>
  {:else if name === "undo"}
    <!--
      Phosphor arrow-counter-clockwise, the same regular weight `marks.ts` carries.

      Scaled rather than redrawn: Phosphor states its outlines on a 256
      box and every drawing above is stated on a 24 one, and 24/256 is
      the whole of the difference. Filled and unstroked inside the
      group, because a regular-weight outline is a filled shape; its
      optical weight is what `marks.ts` says lets the two sets sit in
      one list.
    -->
    <g transform="scale(0.09375)" fill="currentColor" stroke="none">
      <path d="M224,128a96,96,0,0,1-94.71,96H128A95.38,95.38,0,0,1,62.1,197.8a8,8,0,0,1,11-11.63A80,80,0,1,0,71.43,71.39a3.07,3.07,0,0,1-.26.25L44.59,96H72a8,8,0,0,1,0,16H24a8,8,0,0,1-8-8V56a8,8,0,0,1,16,0V85.8L60.25,60A96,96,0,0,1,224,128Z" />
    </g>
  {:else if name === "colour"}
    <!--
      Phosphor eyedropper, the same regular weight `marks.ts` carries.

      Scaled rather than redrawn: Phosphor states its outlines on a 256
      box and every drawing above is stated on a 24 one, and 24/256 is
      the whole of the difference. Filled and unstroked inside the
      group, because a regular-weight outline is a filled shape; its
      optical weight is what `marks.ts` says lets the two sets sit in
      one list.
    -->
    <g transform="scale(0.09375)" fill="currentColor" stroke="none">
      <path d="M224,67.3a35.79,35.79,0,0,0-11.26-25.66c-14-13.28-36.72-12.78-50.62,1.13L142.8,62.2a24,24,0,0,0-33.14.77l-9,9a16,16,0,0,0,0,22.64l2,2.06-51,51a39.75,39.75,0,0,0-10.53,38l-8,18.41A13.68,13.68,0,0,0,36,219.3a15.92,15.92,0,0,0,17.71,3.35L71.23,215a39.89,39.89,0,0,0,37.06-10.75l51-51,2.06,2.06a16,16,0,0,0,22.62,0l9-9a24,24,0,0,0,.74-33.18l19.75-19.87A35.75,35.75,0,0,0,224,67.3ZM97,193a24,24,0,0,1-24,6,8,8,0,0,0-5.55.31l-18.1,7.91L57,189.41a8,8,0,0,0,.25-5.75A23.88,23.88,0,0,1,63,159l51-51,33.94,34ZM202.13,82l-25.37,25.52a8,8,0,0,0,0,11.3l4.89,4.89a8,8,0,0,1,0,11.32l-9,9L112,83.26l9-9a8,8,0,0,1,11.31,0l4.89,4.89a8,8,0,0,0,11.33,0l24.94-25.09c7.81-7.82,20.5-8.18,28.29-.81a20,20,0,0,1,.39,28.7Z" />
    </g>
  {:else if name === "crop"}
    <!--
      Phosphor crop, the same regular weight `marks.ts` carries.

      Scaled rather than redrawn: Phosphor states its outlines on a 256
      box and every drawing above is stated on a 24 one, and 24/256 is
      the whole of the difference. Filled and unstroked inside the
      group, because a regular-weight outline is a filled shape; its
      optical weight is what `marks.ts` says lets the two sets sit in
      one list.
    -->
    <g transform="scale(0.09375)" fill="currentColor" stroke="none">
      <path d="M240,192a8,8,0,0,1-8,8H200v32a8,8,0,0,1-16,0V200H64a8,8,0,0,1-8-8V72H24a8,8,0,0,1,0-16H56V24a8,8,0,0,1,16,0V184H232A8,8,0,0,1,240,192ZM96,72h88v88a8,8,0,0,0,16,0V64a8,8,0,0,0-8-8H96a8,8,0,0,0,0,16Z" />
    </g>
  {:else if name === "screen"}
    <!--
      Phosphor monitor, the same regular weight `marks.ts` carries.

      Scaled rather than redrawn: Phosphor states its outlines on a 256
      box and every drawing above is stated on a 24 one, and 24/256 is
      the whole of the difference. Filled and unstroked inside the
      group, because a regular-weight outline is a filled shape; its
      optical weight is what `marks.ts` says lets the two sets sit in
      one list.
    -->
    <g transform="scale(0.09375)" fill="currentColor" stroke="none">
      <path d="M208,40H48A24,24,0,0,0,24,64V176a24,24,0,0,0,24,24H208a24,24,0,0,0,24-24V64A24,24,0,0,0,208,40Zm8,136a8,8,0,0,1-8,8H48a8,8,0,0,1-8-8V64a8,8,0,0,1,8-8H208a8,8,0,0,1,8,8Zm-48,48a8,8,0,0,1-8,8H96a8,8,0,0,1,0-16h64A8,8,0,0,1,168,224Z" />
    </g>
  {:else if name === "markup"}
    <!--
      Phosphor highlighter, the same regular weight `marks.ts` carries.

      Scaled rather than redrawn: Phosphor states its outlines on a 256
      box and every drawing above is stated on a 24 one, and 24/256 is
      the whole of the difference. Filled and unstroked inside the
      group, because a regular-weight outline is a filled shape; its
      optical weight is what `marks.ts` says lets the two sets sit in
      one list.
    -->
    <g transform="scale(0.09375)" fill="currentColor" stroke="none">
      <path d="M253.66,106.34a8,8,0,0,0-11.32,0L192,156.69,107.31,72l50.35-50.34a8,8,0,1,0-11.32-11.32L96,60.69A16,16,0,0,0,93.18,79.5L72,100.69a16,16,0,0,0,0,22.62L76.69,128,18.34,186.34a8,8,0,0,0,3.13,13.25l72,24A7.88,7.88,0,0,0,96,224a8,8,0,0,0,5.66-2.34L136,187.31l4.69,4.69a16,16,0,0,0,22.62,0l21.19-21.18A16,16,0,0,0,203.31,168l50.35-50.34A8,8,0,0,0,253.66,106.34ZM93.84,206.85l-55-18.35L88,139.31,124.69,176ZM152,180.69,83.31,112,104,91.31,172.69,160Z" />
    </g>
  {:else if name === "key"}
    <!--
      Phosphor key, the same regular weight the marks above are scaled from.

      Every row wearing it is a secret somebody pasted in: an API key, a
      token, a client id. What tells them apart is which service the key is
      for, and that is what the line beside the title says.
    -->
    <g transform="scale(0.09375)" fill="currentColor" stroke="none">
      <path d="M216.57,39.43A80,80,0,0,0,83.91,120.78L28.69,176A15.86,15.86,0,0,0,24,187.31V216a16,16,0,0,0,16,16H72a8,8,0,0,0,8-8V208H96a8,8,0,0,0,8-8V184h16a8,8,0,0,0,5.66-2.34l9.56-9.57A79.73,79.73,0,0,0,160,176h.1A80,80,0,0,0,216.57,39.43ZM224,98.1c-1.09,34.09-29.75,61.86-63.89,61.9H160a63.7,63.7,0,0,1-23.65-4.51,8,8,0,0,0-8.84,1.68L116.69,168H96a8,8,0,0,0-8,8v16H72a8,8,0,0,0-8,8v16H40V187.31l58.83-58.82a8,8,0,0,0,1.68-8.84A63.72,63.72,0,0,1,96,95.92c0-34.14,27.81-62.8,61.9-63.89A64,64,0,0,1,224,98.1ZM192,76a12,12,0,1,1-12-12A12,12,0,0,1,192,76Z" />
    </g>
  {:else if name === "server"}
    <!--
      Phosphor hard-drive: the machine that answers.

      Not a globe, which is `browsers`, not a globe under a lens, which is
      `websearch`, not a chain link, which is `quicklinks`, and not a plug,
      which is `mcp`. Four of the five obvious drawings for "somewhere else"
      were already spoken for, and the honest fifth is the box itself.
    -->
    <g transform="scale(0.09375)" fill="currentColor" stroke="none">
      <path d="M224,64H32A16,16,0,0,0,16,80v96a16,16,0,0,0,16,16H224a16,16,0,0,0,16-16V80A16,16,0,0,0,224,64Zm0,112H32V80H224v96Zm-24-48a12,12,0,1,1-12-12A12,12,0,0,1,200,128Z" />
    </g>
  {:else if name === "clock"}
    <!--
      Phosphor clock. A plain face, where `history` is a face with an arrow
      back round it: this is what time it is, that one is what already
      happened.
    -->
    <g transform="scale(0.09375)" fill="currentColor" stroke="none">
      <path d="M128,24A104,104,0,1,0,232,128,104.11,104.11,0,0,0,128,24Zm0,192a88,88,0,1,1,88-88A88.1,88.1,0,0,1,128,216Zm64-88a8,8,0,0,1-8,8H128a8,8,0,0,1-8-8V72a8,8,0,0,1,16,0v48h48A8,8,0,0,1,192,128Z" />
    </g>
  {:else if name === "weather"}
    <!--
      Phosphor cloud-sun. The sun is what keeps it from reading as a plain
      cloud, which at this size is a shape rather than a forecast.
    -->
    <g transform="scale(0.09375)" fill="currentColor" stroke="none">
      <path d="M164,72a76.2,76.2,0,0,0-20.26,2.73,55.63,55.63,0,0,0-9.41-11.54l9.51-13.57a8,8,0,1,0-13.11-9.18L121.22,54A55.9,55.9,0,0,0,96,48c-.58,0-1.16,0-1.74,0L91.37,31.71a8,8,0,1,0-15.75,2.77L78.5,50.82A56.1,56.1,0,0,0,55.23,65.67L41.61,56.14a8,8,0,1,0-9.17,13.11L46,78.77A55.55,55.55,0,0,0,40,104c0,.57,0,1.15,0,1.72L23.71,108.6a8,8,0,0,0,1.38,15.88,8.24,8.24,0,0,0,1.39-.12l16.32-2.88a55.74,55.74,0,0,0,5.86,12.42A52,52,0,0,0,84,224h80a76,76,0,0,0,0-152ZM56,104a40,40,0,0,1,72.54-23.24,76.26,76.26,0,0,0-35.62,40,52.14,52.14,0,0,0-31,4.17A40,40,0,0,1,56,104ZM164,208H84a36,36,0,1,1,4.78-71.69c-.37,2.37-.63,4.79-.77,7.23a8,8,0,0,0,16,.92,58.91,58.91,0,0,1,1.88-11.81c0-.16.09-.32.12-.48A60.06,60.06,0,1,1,164,208Z" />
    </g>
  {:else if name === "transfer"}
    <!--
      Phosphor tray: things go in and out of it.

      Deliberately neither an up arrow nor a down one. Export and import sit
      next to each other in General and one drawing has to serve both, so a
      direction on it would be wrong half the time.
    -->
    <g transform="scale(0.09375)" fill="currentColor" stroke="none">
      <path d="M208,32H48A16,16,0,0,0,32,48V208a16,16,0,0,0,16,16H208a16,16,0,0,0,16-16V48A16,16,0,0,0,208,32Zm0,16V152h-28.7A15.86,15.86,0,0,0,168,156.69L148.69,176H107.31L88,156.69A15.86,15.86,0,0,0,76.69,152H48V48Zm0,160H48V168H76.69L96,187.31A15.86,15.86,0,0,0,107.31,192h41.38A15.86,15.86,0,0,0,160,187.31L179.31,168H208v40Z" />
    </g>
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

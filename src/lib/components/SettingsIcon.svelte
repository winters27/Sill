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

  /**
   * The fourteen tile hues, each a `--tile-<hue>` token in theme.css.
   *
   * A design decision rather than vendored data, so it lives here beside the
   * names and not in the generated table.
   */
  export const TILE_HUES = [
    "red",
    "orange",
    "amber",
    "olive",
    "green",
    "teal",
    "cyan",
    "blue",
    "indigo",
    "violet",
    "magenta",
    "rose",
    "brown",
    "slate",
  ] as const;

  export type TileHue = (typeof TILE_HUES)[number];

  /**
   * Which hue each mark's tile takes. Families share a hue, so the sidebar
   * reads as a few colours placed with intent rather than forty-five drawn
   * from a hat: the voice panels are one family, the clipboard's kin another,
   * the sources a third. Slate is for the machinery.
   */
  export const HUE: Record<IconName, TileHue> = {
    general: "slate",
    appearance: "indigo",
    ai: "violet",
    dictation: "magenta",
    tts: "rose",
    widgets: "cyan",
    snippets: "orange",
    emoji: "amber",
    shortcuts: "slate",
    quicklinks: "green",
    automations: "olive",
    mcp: "teal",
    clipboard: "amber",
    history: "teal",
    sources: "blue",
    files: "blue",
    browsers: "blue",
    websearch: "blue",
    screenshot: "red",
    extensions: "green",
    scripts: "olive",
    advanced: "brown",
    about: "slate",
    arrangements: "indigo",
    notes: "amber",
    reminders: "rose",
    fonts: "brown",
    displays: "indigo",
    terminals: "slate",
    media: "rose",
    text: "amber",
    picture: "amber",
    confetti: "magenta",
    reindex: "slate",
    undo: "slate",
    colour: "red",
    crop: "red",
    screen: "red",
    markup: "red",
    qr: "slate",
    key: "orange",
    server: "slate",
    clock: "cyan",
    weather: "cyan",
    transfer: "slate",
  };
</script>

<script lang="ts">
  /**
   * The settings icon set: a Phosphor glyph, white, on a coloured tile.
   *
   * ## One kind
   *
   * Every name draws the same way, from `SETTINGS_GLYPHS`, a table generated
   * out of `@phosphor-icons/core` by `scripts/vendor-phosphor.mjs`. The table
   * is typed against `IconName`, so a name with no drawing, or a drawing with
   * no name, fails `npm run check` before it can reach a window. The rendered
   * plaques and the hand-drawn hairline glyphs this replaces were two kinds,
   * and which one a name got depended on whether art had been drawn for it.
   *
   * ## The tile
   *
   * A rounded tile in one of fourteen muted hues, the glyph in white on it.
   * The set before the plaques was seventeen coloured tiles, and it went for
   * being the loudest thing in a window whose whole style is restraint. That
   * is the reason the hues sit at one fixed lightness and a chroma of about
   * a tenth: a column of twenty-one of them has to read as a palette, not a
   * row of buttons. The tokens are in theme.css beside the tile sizes.
   *
   * The tile is the icon's own body and not a surface anything sits on, and
   * no state is read from its hue, which is why a coloured tile does not
   * cross the rule that keeps surfaces neutral and the accent for selection.
   */
  import { SETTINGS_GLYPHS } from "./glyphs";

  interface Props {
    name: IconName;
    /** The tile's box. The glyph sits at 62% of it. */
    size?: number;
  }

  let { name, size = 26 }: Props = $props();

  /**
   * The glyph's box inside the tile.
   *
   * Phosphor's own margin inside its 256 box brings the visible mark to about
   * half the tile, which is the proportion a platform icon keeps.
   */
  const glyph = $derived(Math.round(size * 0.62));

  /**
   * The corner. The launcher's tile radius at the sizes it is used at, and a
   * step up for the settings hero, where the small radius reads as a chip.
   */
  const radius = $derived(size >= 32 ? "var(--radius-md)" : "var(--radius-sm)");
</script>

<span
  class="tile {HUE[name]}"
  style:width="{size}px"
  style:height="{size}px"
  style:border-radius={radius}
  aria-hidden="true"
>
  <svg width={glyph} height={glyph} viewBox="0 0 256 256">
    <path d={SETTINGS_GLYPHS[name]} />
  </svg>
</span>

<style>
  .tile {
    display: grid;
    place-items: center;
    flex: none;
    box-shadow: var(--bevel-tile);
  }

  /* One rule per hue rather than a token name built in the markup, so each
     token is written out where `verify:source` can hold it to theme.css. */
  .red { background-color: var(--tile-red); }
  .orange { background-color: var(--tile-orange); }
  .amber { background-color: var(--tile-amber); }
  .olive { background-color: var(--tile-olive); }
  .green { background-color: var(--tile-green); }
  .teal { background-color: var(--tile-teal); }
  .cyan { background-color: var(--tile-cyan); }
  .blue { background-color: var(--tile-blue); }
  .indigo { background-color: var(--tile-indigo); }
  .violet { background-color: var(--tile-violet); }
  .magenta { background-color: var(--tile-magenta); }
  .rose { background-color: var(--tile-rose); }
  .brown { background-color: var(--tile-brown); }
  .slate { background-color: var(--tile-slate); }

  svg {
    display: block;
    fill: var(--tile-ink);
  }
</style>

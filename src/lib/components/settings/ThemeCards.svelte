<script lang="ts">
  import type { Theme } from "$lib/settings";

  interface Props {
    /** The theme currently in force. */
    value: Theme;
    onpick: (theme: Theme) => void;
  }

  let { value, onpick }: Props = $props();

  /**
   * The themes on offer, in the order they are shown.
   *
   * Names and one-line notes only. Not a colour in sight: the cards render
   * themselves from `[data-theme]`, so this list cannot drift from the
   * palettes the way a table of hex values here would.
   */
  const THEMES: { id: Theme; name: string; note: string }[] = [
    { id: "winters-glass", name: "Winters' Glass", note: "Neutral, blue-grey accent" },
    { id: "oilslick", name: "Oilslick", note: "A faint iridescent wash" },
    { id: "graphite", name: "Graphite", note: "No hue anywhere" },
    { id: "ember", name: "Ember", note: "Warm black, amber accent" },
    { id: "moss", name: "Moss", note: "Cool green" },
    { id: "aberration", name: "Aberration", note: "Warm and cool fringes, like a lens" },
  ];
</script>

<!--
  Each card carries its own `data-theme`, so it is rendered by the palette it
  is offering rather than by a copy of that palette's colours kept here. That
  is why the theme selectors in theme.css are written `[data-theme]` rather
  than `:root[data-theme]`.
-->
<div class="themes" role="radiogroup" aria-label="Theme">
  {#each THEMES as t (t.id)}
    <button
      class="theme"
      class:selected={value === t.id}
      data-theme={t.id}
      role="radio"
      aria-checked={value === t.id}
      onclick={() => onpick(t.id)}
    >
      <!--
        A launcher in miniature, drawn with the real tokens: the query, the
        accent on the letters it matched, the selection wash, the chroma if
        the theme carries one. Three bars used to stand in for all of this,
        and a swatch that abstract showed a canvas and nothing about how the
        theme behaves.
      -->
      <span class="swatch" aria-hidden="true">
        <span class="mini-search">te</span>
        <span class="mini-row lit">
          <span class="mini-icon"></span>
          <span class="mini-title"><mark>Te</mark>rminal</span>
          <span class="mini-kind">App</span>
        </span>
        <span class="mini-row">
          <span class="mini-icon"></span>
          <span class="mini-title"><mark>Te</mark>xt Editor</span>
          <span class="mini-kind">App</span>
        </span>
        <span class="mini-row">
          <span class="mini-icon"></span>
          <span class="mini-title"><mark>Te</mark>mplates</span>
          <span class="mini-kind">Folder</span>
        </span>
      </span>
      <span class="theme-name">{t.name}</span>
      <span class="theme-note">{t.note}</span>
    </button>
  {/each}
</div>

<style>
  /*
   * Wide cards, two or three to a row.
   *
   * They were 148px, sized back when the preview was three bars. A card that
   * narrow cannot hold a legible miniature of the launcher, and the miniature
   * is the whole point of a theme card: what is being chosen is a look, so
   * the card has to be big enough to actually show it.
   */
  .themes {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
    gap: var(--space-3);
  }

  /*
   * Each card is rendered by the theme it offers, so `--core-*` and `--chroma`
   * inside it are that theme's, not the active one's.
   */
  .theme {
    display: flex;
    flex-direction: column;
    align-items: stretch;
    gap: var(--space-2);
    padding: var(--space-2);
    border: 0;
    border-radius: var(--radius-lg);
    background: var(--fill-1);
    box-shadow: var(--ring);
    color: var(--text-1);
    font: inherit;
    text-align: left;
    cursor: pointer;
    transition:
      background-color var(--motion-state) var(--ease),
      box-shadow var(--motion-state) var(--ease);
  }

  .theme:hover {
    background: var(--fill-2);
  }

  /* Selection is the accent, same as everywhere else. The ring is drawn in
     the card's OWN accent, so the chosen theme is showing you its highlight
     colour at the same time as telling you it is chosen. */
  .theme.selected {
    background: var(--fill-2);
    box-shadow: var(--ring-swatch);
  }

  /*
   * A launcher in miniature: the window surface, the query, three rows.
   *
   * Everything in it is painted by the card's own `data-theme`, so what it
   * shows is the theme actually doing its job rather than a legend for it:
   * the matched letters take the accent, the selected row takes the accent
   * wash, and a chroma theme paints across the surface at the strength the
   * slider in Appearance has set. The gradients are sized in percentages, so
   * a card shows the same layout the window does rather than a crop of it.
   */
  .swatch {
    display: flex;
    flex-direction: column;
    gap: var(--space-half);
    padding-bottom: var(--space-1);
    border-radius: var(--radius-md);
    background-color: var(--core-secondary-background);
    background-image: var(--chroma);
    box-shadow: var(--bevel-tile);
    overflow: hidden;
  }

  .mini-search {
    padding: var(--space-2) var(--space-3);
    border-bottom: 1px solid var(--hairline);
    color: var(--text-1);
    font-size: var(--text-micro);
    font-weight: var(--weight-medium);
    letter-spacing: var(--track-micro);
  }

  .mini-row {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    margin: 0 var(--space-1);
    padding: var(--space-1) var(--space-2);
    border-radius: var(--radius-sm);
    font-size: var(--text-micro);
    letter-spacing: var(--track-micro);
  }

  .mini-row.lit {
    background: var(--accent-fill);
  }

  .mini-icon {
    flex: none;
    width: 10px;
    height: 10px;
    border-radius: var(--radius-xs);
    background: var(--fill-3);
  }

  .mini-title {
    min-width: 0;
    color: var(--text-1);
    font-weight: var(--weight-body);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  /* The same rule the real list applies to the letters the ranker matched. */
  .mini-title mark {
    background: none;
    color: var(--core-accent);
    font-weight: var(--weight-strong);
  }

  .mini-kind {
    flex: none;
    margin-left: auto;
    color: var(--text-3);
  }

  .theme-name {
    font-size: var(--text-meta);
    font-weight: var(--weight-medium);
  }

  .theme-note {
    margin-top: calc(var(--space-1) * -1);
    font-size: var(--text-label);
    color: var(--text-3);
  }
</style>

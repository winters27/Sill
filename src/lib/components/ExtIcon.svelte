<script lang="ts">
  /**
   * The icon an extension asked for, in the slot every other row uses.
   *
   * ## Three things, drawn three ways
   *
   * A picture is a picture. A character the extension supplied is printed. A
   * name is a mark, and the marks are Phosphor Icons at regular weight, which
   * is the family this launcher already draws its own menus with.
   *
   * ## Which name is which mark is not decided here
   *
   * Rust decides it, in `src-tauri/src/exthost/icons.rs`. The table itself
   * lives in `./marks.ts` beside this file, and `npm run verify:source` holds
   * the two to each other in every direction they can come apart. See the
   * comment on the Rust table.
   *
   * ## All 469 of them, and the three ways one gets drawn
   *
   * Most are a Phosphor outline, looked up by mark name. A hundred and two are
   * characters in a rounded square, because `Icon.Number42` is a picture of
   * the number forty-two and a hundred hand copies of one drawing is a hundred
   * chances to get one wrong. The last fourteen are drawn below, because they
   * are a quantity rather than a thing: four bars with some of them filled, a
   * ring filled a quarter of the way round, one exclamation mark or three.
   *
   * Those fourteen are stroked at 1.5 on a 24 box, which is the same optical
   * thickness as Phosphor's regular outline on its 256 box, so a list mixing
   * the two reads as one family rather than as two.
   *
   * ## A picture out of the extension's own assets
   *
   * `icon: "files.png"` means the file beside the extension's code. The
   * window cannot open it and does not know where the extension lives, so it
   * asks Rust by session (`$lib/exthost/assets`), which finds the extension,
   * refuses a name that climbs, and reads the picture. Until the answer is
   * back the tile is reserved and empty rather than lettered, so a row drawn
   * on the next keystroke does not flash a letter under a picture that was
   * already known. A name Rust has no picture for letters itself, which is
   * the launcher's existing answer for an application whose icon the shell
   * will not give up, so it looks like something Sill drew rather than like
   * something that failed.
   */
  import { getContext } from "svelte";
  import type { ExtIcon } from "$lib/exthost/present";
  import { VIEW_SESSION, extensionAsset, knownAsset, type SessionOf } from "$lib/exthost/assets";
  import { GLYPHS, MARKS, TEXT_MARKS } from "./marks";

  interface Props {
    icon: ExtIcon;
    /** Small enough to sit inside an accessory pill, when it has to. */
    small?: boolean;
    /**
     * What this is an icon of, for the tile a failed picture falls back to.
     *
     * Only a picture needs it: a name is its own label and a character is
     * already the thing being drawn. Absent where there is nothing sensible
     * to letter, which is an accessory pill: an "A" beside a number, standing
     * in for an arrow that would not load, is worse than the arrow's absence.
     */
    label?: string;
  }

  let { icon, small = false, label }: Props = $props();

  const drawn = $derived(icon.kind === "mark" ? (MARKS[icon.name] ?? "") : "");

  /** The outline, when the mark is one of the ones Phosphor has. */
  const outline = $derived(GLYPHS[drawn] ?? "");

  /** The characters, when the mark is a numeral or a letter pair. */
  const printed = $derived(TEXT_MARKS[drawn] ?? "");

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

  /** Which session this view belongs to, from the page that hosts it. */
  const sessionOf = getContext<SessionOf | undefined>(VIEW_SESSION);

  /** The asset's picture: unknown yet, the data URI, or null for none. */
  let asset = $state<string | null | undefined>(undefined);

  $effect(() => {
    if (icon.kind !== "asset") {
      asset = undefined;
      return;
    }
    const session = sessionOf?.() ?? null;
    const name = icon.name;
    const known = knownAsset(session, name);
    asset = known;
    if (known !== undefined) return;
    void extensionAsset(session, name).then((got) => {
      // Still the same picture being asked for; a row can be reused for
      // another icon before the first answer is back.
      if (icon.kind === "asset" && icon.name === name) asset = got;
    });
  });

  /** The picture to put in the img, whichever way it arrived. */
  const picture = $derived(
    icon.kind === "image" ? icon.src : icon.kind === "asset" ? (asset ?? null) : null,
  );

  /** Whether the asset is still being fetched, which is a tile to reserve. */
  const waiting = $derived(icon.kind === "asset" && asset === undefined);

  const broke = $derived(
    (icon.kind === "image" && refused === icon.src) ||
      (icon.kind === "asset" && (asset === null || (asset !== undefined && refused === asset))),
  );

  /**
   * The letter on the tile, for the two things that have no picture to draw.
   *
   * A name with no mark letters itself, which is the launcher's existing
   * answer and was already here. **A picture that would not load letters its
   * label**, which was not: the comment above `refused` says a broken image
   * "falls back to the letter tile", and it reached the tile with no letter
   * in it, so a row whose icon failed drew an empty grey square. That is the
   * one outcome worse than either alternative, because it looks like a
   * decision rather than a failure.
   */
  const initial = $derived.by(() => {
    if (icon.kind === "mark") return (icon.name.trim()[0] ?? "?").toUpperCase();
    if (broke) {
      // The label, or the file's own name: "s" for send.png is a truer
      // stand-in than nothing.
      const own = icon.kind === "asset" ? icon.name.trim()[0] : undefined;
      return (label?.trim()[0] ?? own ?? "").toUpperCase();
    }
    return "";
  });

  /**
   * A failed picture with nothing to letter is drawn as nothing at all.
   *
   * An empty tile is a shape somebody has to interpret. A row with no icon is
   * a row with no tile, and a picture that did not arrive is closer to that
   * than to a blank one somebody might read as the icon itself.
   */
  const nothing = $derived(broke && !initial);
</script>

{#if !nothing}
  <span
    class="ext-icon"
    class:small
    class:lettered={(icon.kind === "mark" && !drawn) || broke}
    style={icon.tint ? `color: ${icon.tint}` : undefined}
  >
    {#if picture && !broke}
      <img src={picture} alt="" onerror={() => (refused = picture)} />
    {:else if waiting}
      <!-- Reserved, not lettered: the picture is on its way. -->
    {:else if icon.kind === "glyph"}
      <span class="glyph">{icon.text}</span>
    {:else if outline}
      <!--
        Phosphor's regular weight is a filled outline rather than a stroked
        one, so the box carries `fill` and no stroke settings at all: nothing
        it sits inside can thicken it by inheriting one.
      -->
      <svg viewBox="0 0 256 256" fill="currentColor" aria-hidden="true">
        <path d={outline} />
      </svg>
    {:else if printed}
      <svg
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="1.5"
        stroke-linejoin="round"
        aria-hidden="true"
      >
        <rect x="3.2" y="3.2" width="17.6" height="17.6" rx="4.6" />
        <text
          x="12"
          y="12.5"
          text-anchor="middle"
          dominant-baseline="middle"
          font-size="9.4"
          font-weight="600"
          fill="currentColor"
          stroke="none">{printed}</text
        >
      </svg>
    {:else if drawn}
      <svg
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="1.5"
        stroke-linecap="round"
        stroke-linejoin="round"
        aria-hidden="true"
      >
        <!--
          A quantity, not a thing. Four bars with the first few lit, which is
          how a strength reading is read everywhere else; the unlit ones stay
          on the drawing because the gap between two bars and four is the
          whole message.

          Raycast's signal names land here too. Phosphor has a cell-signal
          family and its "none" is an antenna base and nothing else, which at
          sixteen pixels is a speck on an otherwise empty tile: the reading
          that most needs to be legible drawn as almost nothing.
        -->
        {#if drawn === "bars-0"}
          <path stroke-width="2.6" opacity=".28" d="M4.8 20v-3.4M9.2 20v-6.8M13.6 20v-10.2M18 20v-13.6" />
        {:else if drawn === "bars-1"}
          <path stroke-width="2.6" d="M4.8 20v-3.4" />
          <path stroke-width="2.6" opacity=".28" d="M9.2 20v-6.8M13.6 20v-10.2M18 20v-13.6" />
        {:else if drawn === "bars-2"}
          <path stroke-width="2.6" d="M4.8 20v-3.4M9.2 20v-6.8" />
          <path stroke-width="2.6" opacity=".28" d="M13.6 20v-10.2M18 20v-13.6" />
        {:else if drawn === "bars-3"}
          <path stroke-width="2.6" d="M4.8 20v-3.4M9.2 20v-6.8M13.6 20v-10.2" />
          <path stroke-width="2.6" opacity=".28" d="M18 20v-13.6" />
        {:else if drawn === "bars-4"}
          <!-- Four of four draws no faint bars, which is the point of it. -->
          <path stroke-width="2.6" d="M4.8 20v-3.4M9.2 20v-6.8M13.6 20v-10.2M18 20v-13.6" />
        {:else if drawn === "progress-0"}
          <circle cx="12" cy="12" r="8" opacity=".28" />
        {:else if drawn === "progress-25"}
          <circle cx="12" cy="12" r="8" opacity=".28" />
          <path d="M12 4a8 8 0 0 1 8 8" />
        {:else if drawn === "progress-50"}
          <circle cx="12" cy="12" r="8" opacity=".28" />
          <path d="M12 4a8 8 0 0 1 0 16" />
        {:else if drawn === "progress-75"}
          <circle cx="12" cy="12" r="8" opacity=".28" />
          <path d="M12 4a8 8 0 0 1 0 16 8 8 0 0 1-8-8" />
        {:else if drawn === "progress-100"}
          <circle cx="12" cy="12" r="8" />
        {:else if drawn === "shout-1"}
          <path d="M12 5.4v8.4" />
          <circle cx="12" cy="17.8" r="1.1" fill="currentColor" stroke="none" />
        {:else if drawn === "shout-2"}
          <path d="M9.2 5.4v8.4M14.8 5.4v8.4" />
          <circle cx="9.2" cy="17.8" r="1.1" fill="currentColor" stroke="none" />
          <circle cx="14.8" cy="17.8" r="1.1" fill="currentColor" stroke="none" />
        {:else if drawn === "shout-3"}
          <path d="M6.8 5.4v8.4M12 5.4v8.4M17.2 5.4v8.4" />
          <circle cx="6.8" cy="17.8" r="1.1" fill="currentColor" stroke="none" />
          <circle cx="12" cy="17.8" r="1.1" fill="currentColor" stroke="none" />
          <circle cx="17.2" cy="17.8" r="1.1" fill="currentColor" stroke="none" />
        {:else if drawn === "chess-piece"}
          <circle cx="12" cy="7" r="2.6" />
          <path d="M9.7 10.3h4.6l-1 1.9c1.7 1.3 2.7 3.2 2.9 5.1H7.8c.2-1.9 1.2-3.8 2.9-5.1Z" />
          <path d="M6.6 17.3h10.8v2.3H6.6Z" />
        {:else if drawn === "star-disabled"}
          <!--
            The star kept and struck through rather than swapped for another
            shape: a plain star would say the row is starred, which is the
            opposite of what the name asks for.
          -->
          <path
            opacity=".4"
            d="M12 4.5 14.3 9.4l5.2.7-3.8 3.7.9 5.2-4.6-2.5-4.6 2.5.9-5.2L4.5 10l5.2-.7Z"
          />
          <path d="m5.2 5.2 13.6 13.6" />
        {/if}
      </svg>
    {:else}
      <span class="initial">{initial}</span>
    {/if}
  </span>
{/if}

<style>
  .ext-icon {
    display: grid;
    flex: none;
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

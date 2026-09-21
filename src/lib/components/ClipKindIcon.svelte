<script lang="ts">
  /**
   * The mark for a kind of clipboard entry: a Phosphor glyph from the
   * generated table, in the row's own colour.
   *
   * Bare rather than on a tile, for three reasons. The colour kind's swatch
   * below is itself a coloured square, so a tiled link row and a red colour
   * entry would be the same shape at fourteen pixels. A colour per kind in a
   * dense list is a legend to learn, and the type filter already spells the
   * kind out beside the glyph. And menus keep monochrome marks, which the
   * launcher menu decided for the same reason.
   */
  import type { ClipKind } from "$lib/clipboard";
  import { CLIP_GLYPHS } from "$lib/components/glyphs";

  interface Props {
    kind: ClipKind;
    /** A colour entry shows its own colour rather than a glyph. */
    swatch?: string;
    size?: number;
  }

  let { kind, swatch, size = 15 }: Props = $props();
</script>

{#if kind === "color" && swatch}
  <!-- The one kind whose content IS its icon. -->
  <span class="swatch" style:background={swatch} style:width="{size}px" style:height="{size}px"
  ></span>
{:else}
  <svg width={size} height={size} viewBox="0 0 256 256" fill="currentColor" aria-hidden="true">
    <path d={CLIP_GLYPHS[kind]} />
  </svg>
{/if}

<style>
  .swatch {
    display: block;
    flex: none;
    border-radius: var(--radius-sm);
    /* An inset edge rather than a border, so a white swatch still reads as a
       square on a dark row instead of dissolving into it. */
    box-shadow: var(--ring-bright);
  }
</style>

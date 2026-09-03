<script lang="ts">
  import type { ClipKind } from "$lib/clipboard";

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
  <svg
    width={size}
    height={size}
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    stroke-width="1.7"
    stroke-linecap="round"
    stroke-linejoin="round"
    aria-hidden="true"
  >
    {#if kind === "link"}
      <path d="M9.5 14.5a3.5 3.5 0 0 0 5 0l3-3a3.54 3.54 0 0 0-5-5l-1 1" />
      <path d="M14.5 9.5a3.5 3.5 0 0 0-5 0l-3 3a3.54 3.54 0 0 0 5 5l1-1" />
    {:else if kind === "email"}
      <rect x="2.5" y="5" width="19" height="14" rx="2" />
      <path d="m3 7 9 6 9-6" />
    {:else if kind === "image"}
      <rect x="2.5" y="4" width="19" height="16" rx="2" />
      <circle cx="8.5" cy="9.5" r="1.6" />
      <path d="m3 16 5-4 5 4 3-2.5 5 4" />
    {:else if kind === "file"}
      <path d="M13.5 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8.5Z" />
      <path d="M13.5 3v5.5H19" />
    {:else}
      <!-- Text: lines on a page, which is what a copied paragraph is. -->
      <path d="M5 5h14M5 9.5h14M5 14h10M5 18.5h7" />
    {/if}
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

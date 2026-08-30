<script lang="ts" module>
  export type MarkIcon =
    | "select"
    | "box"
    | "arrow"
    | "ellipse"
    | "pen"
    | "highlight"
    | "hide"
    | "text"
    | "step"
    | "crop"
    | "undo"
    | "clear"
    | "close"
    | "copy";
</script>

<script lang="ts">
  /**
   * The markup editor's marks.
   *
   * Drawn rather than lettered. A row of words is a row of things to read; a
   * row of marks is a row of things to recognise, and a tool bar is looked at
   * far more often than it is read.
   *
   * One stroke width and one join style across the set, so they sit together
   * as a family rather than as eight drawings that happen to be the same size.
   */
  interface Props {
    name: MarkIcon;
    size?: number;
  }

  let { name, size = 16 }: Props = $props();
</script>

<svg
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width="1.75"
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"
>
  {#if name === "select"}
    <!-- A pointer, which is what "stop drawing and pick something up" looks
         like everywhere. -->
    <path d="M5 3 19 11.5 12.5 13 10 19.5Z" />
  {:else if name === "box"}
    <rect x="4" y="6" width="16" height="12" rx="1.5" />
  {:else if name === "arrow"}
    <path d="M5 19 19 5" />
    <path d="M11 5h8v8" />
  {:else if name === "ellipse"}
    <ellipse cx="12" cy="12" rx="8" ry="6" />
  {:else if name === "pen"}
    <path d="M4 20c2-1 3-3.5 5-6s4-5.5 6-6.5 4 0 3 2.5-4 5-6.5 6.5S6 20 4 20Z" />
  {:else if name === "highlight"}
    <!-- A marker nib with its stroke under it. -->
    <path d="M9 14 15.5 7.5a2 2 0 0 1 3 3L12 17H9Z" />
    <path d="M4 20h16" stroke-width="3" />
  {:else if name === "hide"}
    <!-- Blocks, which is what this actually does to the pixels. -->
    <rect x="4" y="7" width="5" height="5" />
    <rect x="14" y="7" width="5" height="5" />
    <rect x="9" y="12" width="5" height="5" />
  {:else if name === "text"}
    <path d="M5 6h14" />
    <path d="M12 6v13" />
  {:else if name === "step"}
    <!-- A numbered disc, which is what the tool drops. -->
    <circle cx="12" cy="12" r="8" />
    <path d="M10.5 9.5 12.5 8v8" />
  {:else if name === "crop"}
    <!-- The two overlapping rules every crop tool is drawn as. -->
    <path d="M7 3v14h14" />
    <path d="M3 7h14v14" />
  {:else if name === "undo"}
    <path d="M4 10h10a5 5 0 0 1 0 10h-4" />
    <path d="M8 6 4 10l4 4" />
  {:else if name === "clear"}
    <path d="M5 7h14" />
    <path d="M9 7V5h6v2" />
    <path d="M7 7v12a1 1 0 0 0 1 1h8a1 1 0 0 0 1-1V7" />
  {:else if name === "close"}
    <path d="M7 7 17 17" />
    <path d="M17 7 7 17" />
  {:else if name === "copy"}
    <rect x="9" y="4" width="11" height="13" rx="1.5" />
    <path d="M15 20H5a1 1 0 0 1-1-1V8" />
  {/if}
</svg>

<style>
  svg {
    display: block;
    flex: none;
  }
</style>

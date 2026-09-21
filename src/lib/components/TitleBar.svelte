<script lang="ts">
  import type { Snippet } from "svelte";
  import { getCurrentWindow } from "@tauri-apps/api/window";

  interface Props {
    /** Optional. The pane below already names what is on screen. */
    title?: string;
    /**
     * Controls that live in the bar, drawn between the title and the
     * caption buttons. A window that has none passes nothing.
     */
    children?: Snippet;
  }

  let { title = "", children }: Props = $props();

  const win = getCurrentWindow();
</script>

<!--
  A frameless window has to provide its own drag region and controls.

  `data-tauri-drag-region` is what makes the bar behave like a title bar; the
  buttons opt out of it so a click on close does not start a drag instead.

  The bar floats. It is out of the flow, over the top of whatever the window
  draws, so a sidebar's hairline runs to the corner and a transcript scrolls
  under the chrome. A bar in the flow owns its band whether or not it paints
  one, and that band read as a strip of a different colour across the top of
  every window. The pages pad their content by `--titlebar-height`.
-->
<div class="bar" data-tauri-drag-region>
  <!--
    The mark, so the window says whose it is.

    `data-tauri-drag-region` on the image as well as the bar: a child without it
    is a hole in the drag region, and a 20px dead spot in the corner somebody
    grabs a window by is the kind of thing that reads as the app being stuck.
  -->
  <img
    class="mark"
    src="/sill.png"
    alt=""
    width="20"
    height="20"
    draggable="false"
    data-tauri-drag-region
  />

  {#if title}<span class="title" data-tauri-drag-region>{title}</span>{/if}

  <span class="spacer" data-tauri-drag-region></span>

  <!--
    The window's own controls sit here. A control carries no drag attribute,
    so a press on it is a press and not the start of a drag, while the gaps
    around it still drag.
  -->
  {#if children}
    <span class="controls" data-tauri-drag-region>{@render children()}</span>
  {/if}

  <button class="control" aria-label="Minimise" onclick={() => win.minimize()}>
    <!--
      A 10-unit grid drawn at 10px, so one unit is one device pixel. The
      one weight that is crisp there is 1; `--stroke-glyph` is tuned for a
      24-unit grid drawn at 16, where it lands soft on purpose.
    -->
    <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
      <path d="M0 5h10" stroke="currentColor" stroke-width="1" />
    </svg>
  </button>

  <button class="control close" aria-label="Close" onclick={() => win.close()}>
    <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
      <path d="M0 0l10 10M10 0L0 10" stroke="currentColor" stroke-width="1" />
    </svg>
  </button>
</div>

<style>
  .bar {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    z-index: var(--z-raised);
    display: flex;
    align-items: center;
    gap: var(--space-2);
    height: var(--titlebar-height);
    padding-left: var(--space-4);
  }

  .mark {
    flex: none;
    -webkit-user-drag: none;
  }

  .title {
    font-size: var(--text-meta);
    color: var(--text-2);
    pointer-events: none;
  }

  .spacer {
    flex: 1;
    align-self: stretch;
  }

  .controls {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    align-self: stretch;
    padding-right: var(--space-2);
  }

  /*
    Caption buttons, at the platform's geometry rather than the app's.

    These are the WINDOW's controls, so they follow the OS convention instead
    of the capsule language the rest of the bar speaks. The point is aim, not
    decoration: Windows puts close in the literal corner, the one target you
    can hit by slamming the pointer at it, so these are the full height of
    the bar, 46 wide, no radius, no gap, flush to the edge.
  */
  .control {
    display: grid;
    place-items: center;
    width: 46px;
    height: var(--titlebar-height);
    border: 0;
    background: transparent;
    color: var(--text-2);
    cursor: pointer;
    transition:
      background-color var(--motion-state) var(--ease),
      color var(--motion-state) var(--ease);
  }

  .control:hover {
    background-color: var(--fill-2);
    color: var(--text-1);
  }

  /* The one control that is destructive gets the one colour that says so. */
  .control.close:hover {
    background-color: var(--titlebar-close);
    color: var(--text-1);
  }
</style>

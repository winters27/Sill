<script lang="ts">
  import { getCurrentWindow } from "@tauri-apps/api/window";

  interface Props {
    /** Optional. The pane below already names what is on screen. */
    title?: string;
  }

  let { title = "" }: Props = $props();

  const win = getCurrentWindow();
</script>

<!--
  A frameless window has to provide its own drag region and controls.

  `data-tauri-drag-region` is what makes the bar behave like a title bar; the
  buttons opt out of it so a click on close does not start a drag instead.
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

  <button class="control" aria-label="Minimise" onclick={() => win.minimize()}>
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
    display: flex;
    align-items: center;
    gap: var(--space-2);
    height: 42px;
    padding-left: var(--space-4);
    flex: none;
    border-bottom: 1px solid var(--hairline);
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

  .control {
    display: grid;
    place-items: center;
    width: 46px;
    height: 42px;
    border: 0;
    background: transparent;
    color: var(--text-2);
    cursor: pointer;
    transition: background-color 0.15s var(--ease), color 0.15s var(--ease);
  }

  .control:hover {
    background-color: var(--fill-2);
    color: var(--text-1);
  }

  /* The one control that is destructive gets the one colour that says so. */
  .control.close:hover {
    background-color: #c42b1c;
    color: #ffffff;
  }
</style>

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
    height: 38px;
    padding-left: 16px;
    flex: none;
    border-bottom: 1px solid var(--hairline);
  }

  .title {
    font-size: 12px;
    color: var(--text-muted);
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
    height: 38px;
    border: 0;
    background: transparent;
    color: var(--text-muted);
    cursor: pointer;
    transition: background-color 0.15s var(--ease), color 0.15s var(--ease);
  }

  .control:hover {
    background-color: rgba(var(--accent-rgb), 0.12);
    color: var(--core-foreground);
  }

  /* The one control that is destructive gets the one colour that says so. */
  .control.close:hover {
    background-color: #c42b1c;
    color: #ffffff;
  }
</style>

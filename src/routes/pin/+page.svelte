<script lang="ts">
  /**
   * A picture left floating over everything else.
   *
   * The thing you do when you want to keep looking at one window while working
   * in another, and the reason it is a window rather than a mode: it has to
   * survive switching to the program the comparison is with, which nothing
   * drawn inside Sill can.
   *
   * **Almost nothing happens here.** The window's size, its position and its
   * closing are all Rust's, so this page is granted no way to size, place or
   * close itself and cannot be talked into moving a picture of somebody's
   * screen somewhere they cannot see it. What is left is the drag, which has
   * no other mechanism, and the wheel, which asks Rust for a new size and is
   * told what it actually got.
   */
  import { onMount } from "svelte";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { closePin, pinImage, scalePin } from "$lib/capture";
  import { applyAppearance, getPreferences } from "$lib/settings";
  import { silently } from "$lib/status";
  import "$lib/theme/theme.css";

  let uri = $state<string | null>(null);
  let scale = $state(1);

  async function load() {
    uri = await pinImage();
  }

  /**
   * Bigger or smaller, a step at a time.
   *
   * Rust clamps and answers with what it applied, so the number here is what
   * the window actually is rather than what was asked for. Reading it back is
   * what stops the wheel running away past a bound that silently refused it.
   */
  async function wheel(event: WheelEvent) {
    event.preventDefault();

    const wanted = scale * (event.deltaY < 0 ? 1.1 : 1 / 1.1);
    scale = await scalePin(wanted);
  }

  function key(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      void closePin();
    }
  }

  onMount(() => {
    let off: UnlistenFn | undefined;

    // The same theme as every other window. There is no global stylesheet in
    // this application: a route that does not ask for the tokens runs on the
    // stylesheet's fallbacks, which is how the markup window once drew an
    // invisible button.
    void getPreferences().then(applyAppearance).catch(silently(undefined));

    void load();

    // Shown again with a different picture, which is one window rather than a
    // new one each time, so the page is told rather than asked.
    void listen("sill://pin", () => {
      scale = 1;
      void load();
    }).then((stop) => (off = stop));

    return () => off?.();
  });
</script>

<svelte:window onkeydown={key} />

<!--
  The drag region is the picture itself. There is no title bar to grab: a
  chrome-less rectangle is the whole point, and a bar over the top would cover
  the thing somebody pinned in order to look at.
-->
<div
  class="pin"
  data-tauri-drag-region
  onwheel={wheel}
  ondblclick={() => void closePin()}
  role="presentation"
>
  {#if uri}
    <img src={uri} alt="What was pinned" data-tauri-drag-region draggable="false" />
  {/if}
</div>

<style>
  /*
    Pinned rather than `100vh`. On a frameless window those are not reliably
    the same number, which is what once cut the markup window's footer off
    along the bottom edge.
  */
  .pin {
    position: fixed;
    inset: 0;
    overflow: hidden;
    background: var(--core-background);
    cursor: grab;
  }

  .pin:active {
    cursor: grabbing;
  }

  /*
    Filling the window rather than sized here. Rust makes the window the
    picture's shape, so one of these two has to be the authority and the one
    holding the real dimensions is the better choice.
  */
  img {
    display: block;
    width: 100%;
    height: 100%;
    /* Or dragging the picture starts a selection instead of moving the
       window, which is the one gesture this window has. */
    -webkit-user-select: none;
    user-select: none;
  }
</style>

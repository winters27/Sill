<script lang="ts">
  /**
   * Picking a piece of the screen.
   *
   * The whole window is the screen, sized and placed in physical pixels by
   * Rust, so a rectangle drawn here is a rectangle of the screen. The only
   * conversion is the display's scaling: the pointer reports logical pixels
   * and the screen is copied in physical ones.
   *
   * The dimming is what makes the selection readable. Rather than drawing a
   * dark sheet and cutting a hole in it, which needs a mask and repaints the
   * whole screen every frame, four panels are laid around the selection. The
   * area being picked has nothing over it at all, so it stays exactly the
   * colour it will be in the picture.
   */
  import { onMount } from "svelte";
  import "$lib/theme/theme.css";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import {
    cancelCapture,
    captureArea,
    captureTargets,
    captureWindow,
    type CaptureTarget,
  } from "$lib/capture";
  import { windowUnder } from "$lib/markup";
  import { getPreferences } from "$lib/settings";

  /** Where the drag started, in this window's own pixels. */
  let from = $state<{ x: number; y: number } | null>(null);
  let to = $state<{ x: number; y: number } | null>(null);
  /** Set once the pointer has actually moved, so a click is not a capture. */
  let dragging = $state(false);
  let status = $state("");

  /** The windows a click could take, topmost first. */
  let targets = $state<CaptureTarget[]>([]);
  /** Whichever of them the pointer is over, in this window's own pixels. */
  let hovering = $state<{ target: CaptureTarget; box: Box } | null>(null);
  let clickAWindow = $state(true);
  /** The scale and origin needed to convert both ways. Read once on show. */
  let frame = $state({ scale: 1, x: 0, y: 0 });

  interface Box {
    left: number;
    top: number;
    width: number;
    height: number;
  }

  /** The rectangle being picked, normalised so any drag direction works. */
  const picked = $derived.by(() => {
    if (!from || !to) return null;

    return {
      left: Math.min(from.x, to.x),
      top: Math.min(from.y, to.y),
      width: Math.abs(to.x - from.x),
      height: Math.abs(to.y - from.y),
    };
  });

  /**
   * The smallest drag that counts.
   *
   * Below this it is a click, and a click should cancel rather than capture a
   * three pixel picture nobody meant to take.
   */
  const ENOUGH = 8;

  function down(event: PointerEvent) {
    if (event.button !== 0) {
      void cancel();
      return;
    }

    from = { x: event.clientX, y: event.clientY };
    to = { x: event.clientX, y: event.clientY };
    dragging = true;
  }

  function move(event: PointerEvent) {
    if (dragging) {
      to = { x: event.clientX, y: event.clientY };
      // A drag is a drag: nothing is being pointed at any more.
      hovering = null;
      return;
    }

    if (!clickAWindow) return;

    // The pointer is in this window's pixels and the windows are in the
    // screen's, so the comparison happens in the screen's.
    const found = windowUnder(targets, {
      x: frame.x + event.clientX * frame.scale,
      y: frame.y + event.clientY * frame.scale,
    });

    hovering = found
      ? {
          target: found,
          box: {
            left: (found.left - frame.x) / frame.scale,
            top: (found.top - frame.y) / frame.scale,
            width: found.width / frame.scale,
            height: found.height / frame.scale,
          },
        }
      : null;
  }

  async function up() {
    if (!dragging) return;
    dragging = false;

    const area = picked;
    from = null;
    to = null;

    if (!area || area.width < ENOUGH || area.height < ENOUGH) {
      // Too small to be a drag, so it was a click. On a window, that means
      // take the window; anywhere else it means somebody changed their mind.
      const under = hovering?.target;
      if (under) {
        try {
          status = await captureWindow(under.id);
        } catch (err) {
          status = `${err}`;
        }
        return;
      }

      await cancel();
      return;
    }

    /*
     * Logical to physical.
     *
     * The pointer reports pixels the way the page sees them; the screen is
     * copied in the pixels it actually has. On a 150% display those differ by
     * half again, and without this the picture is the wrong part of the screen
     * and the wrong size, which reads as the capture being subtly broken
     * rather than as a units mistake.
     */
    const scale = await getCurrentWindow().scaleFactor();
    const position = await getCurrentWindow().outerPosition();

    try {
      status = await captureArea(
        position.x + Math.round(area.left * scale),
        position.y + Math.round(area.top * scale),
        Math.round(area.width * scale),
        Math.round(area.height * scale),
      );
    } catch (err) {
      status = `${err}`;
    }
  }

  async function cancel() {
    from = null;
    to = null;
    dragging = false;
    await cancelCapture();
  }

  function key(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      void cancel();
    }
  }

  onMount(() => {
    // Every time it is shown, not only the first: the window is hidden and
    // shown again rather than made afresh, so a stale drag would survive.
    const window = getCurrentWindow();
    const unlisten = window.onFocusChanged(({ payload }) => {
      if (payload) {
        from = null;
        to = null;
        dragging = false;
        hovering = null;
        status = "";
        void ready();
      }
    });

    void ready();

    /** Everything the overlay needs to know, read fresh each time it opens. */
    async function ready() {
      const [scale, position, prefs] = await Promise.all([
        window.scaleFactor(),
        window.outerPosition(),
        getPreferences().catch(() => null),
      ]);

      frame = { scale, x: position.x, y: position.y };
      clickAWindow = prefs?.screenshot?.clickAWindow ?? true;
      targets = clickAWindow ? await captureTargets() : [];
    }

    return () => {
      void unlisten.then((off) => off());
    };
  });
</script>

<svelte:window onkeydown={key} />

<!--
  One surface covering every screen. `pointerdown` starts a drag anywhere on
  it, and the four dimming panels are drawn underneath the pointer handling
  rather than over it, so a drag that begins inside the dimmed area still works.
-->
<div
  class="sheet"
  role="presentation"
  onpointerdown={down}
  onpointermove={move}
  onpointerup={up}
>
  {#if picked}
    <!-- Around the selection, never over it: the area being picked has to
         stay the colour it will actually be in the picture. -->
    <div class="dim" style:inset="0 0 auto 0" style:height="{picked.top}px"></div>
    <div
      class="dim"
      style:top="{picked.top}px"
      style:height="{picked.height}px"
      style:left="0"
      style:width="{picked.left}px"
    ></div>
    <div
      class="dim"
      style:top="{picked.top}px"
      style:height="{picked.height}px"
      style:left="{picked.left + picked.width}px"
      style:right="0"
    ></div>
    <div class="dim" style:top="{picked.top + picked.height}px" style:inset-inline="0" style:bottom="0"></div>

    <div
      class="picked"
      style:left="{picked.left}px"
      style:top="{picked.top}px"
      style:width="{picked.width}px"
      style:height="{picked.height}px"
    ></div>

    <!-- The size, put on whichever side of the selection has room, so it is
         never off the screen at the edges. -->
    <div
      class="size"
      style:left="{picked.left}px"
      style:top="{picked.top > 28 ? picked.top - 24 : picked.top + picked.height + 6}px"
    >
      {picked.width} x {picked.height}
    </div>
  {:else if hovering}
    <!-- The window under the pointer, lit the same way a dragged area is, so
         the two ways of choosing look like one thing. -->
    <div class="dim" style:inset="0 0 auto 0" style:height="{hovering.box.top}px"></div>
    <div
      class="dim"
      style:top="{hovering.box.top}px"
      style:height="{hovering.box.height}px"
      style:left="0"
      style:width="{hovering.box.left}px"
    ></div>
    <div
      class="dim"
      style:top="{hovering.box.top}px"
      style:height="{hovering.box.height}px"
      style:left="{hovering.box.left + hovering.box.width}px"
      style:right="0"
    ></div>
    <div
      class="dim"
      style:top="{hovering.box.top + hovering.box.height}px"
      style:inset-inline="0"
      style:bottom="0"
    ></div>

    <div
      class="picked"
      style:left="{hovering.box.left}px"
      style:top="{hovering.box.top}px"
      style:width="{hovering.box.width}px"
      style:height="{hovering.box.height}px"
    ></div>

    <div
      class="size"
      style:left="{hovering.box.left}px"
      style:top={hovering.box.top > 28
        ? `${hovering.box.top - 24}px`
        : `${hovering.box.top + hovering.box.height + 6}px`}
    >
      {hovering.target.app || hovering.target.title}
    </div>
  {:else}
    <div class="dim" style:inset="0"></div>
    <p class="hint">
      {#if clickAWindow}
        Drag to pick an area, or click a window.
      {:else}
        Drag to pick an area.
      {/if}
      <span class="sill-key">Esc</span> to cancel.
    </p>
  {/if}

  {#if status}
    <p class="status">{status}</p>
  {/if}
</div>

<style>
  .sheet {
    position: fixed;
    inset: 0;
    cursor: crosshair;
    /* Nothing here should be selectable, and a drag that starts a text
       selection fights the drag that picks the area. */
    user-select: none;
    -webkit-user-select: none;
  }

  .dim {
    position: fixed;
    background: rgb(0 0 0 / 0.45);
  }

  .picked {
    position: fixed;
    /* An outline rather than a border: a border would sit inside the rectangle
       and cover the first pixel of what is being picked. */
    outline: 1px solid var(--accent, #4a9eff);
    /* The selection reads as a hole in the dimming, so it must not be tinted. */
    background: transparent;
  }

  .size {
    position: fixed;
    padding: 2px 6px;
    border-radius: 4px;
    background: rgb(0 0 0 / 0.75);
    color: #fff;
    font: 12px/1.4 system-ui, sans-serif;
    font-variant-numeric: tabular-nums;
    pointer-events: none;
  }

  .hint,
  .status {
    position: fixed;
    left: 50%;
    transform: translateX(-50%);
    margin: 0;
    padding: 8px 16px;
    border-radius: 8px;
    background: rgb(0 0 0 / 0.75);
    color: #fff;
    font: 13px/1.4 system-ui, sans-serif;
    pointer-events: none;
  }

  .hint {
    top: 48px;
    display: flex;
    gap: var(--space-1);
    align-items: center;
  }

  /*
   * The overlay is drawn over the desktop rather than over Sill's own
   * background, so the cap needs a ground of its own. Everything else here
   * carries its own colour for the same reason.
   */
  .hint :global(.sill-key) {
    background: rgb(255 255 255 / 0.16);
    box-shadow: none;
    color: #fff;
  }

  .status {
    bottom: 48px;
  }
</style>

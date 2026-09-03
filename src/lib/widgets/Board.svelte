<script lang="ts">
  import Clock from "./Clock.svelte";
  import Weather from "./Weather.svelte";
  import Machine from "./Machine.svelte";
  import { WIDGETS } from "./registry";
  import type { Preferences } from "$lib/settings";
  import { hint } from "$lib/hint";

  interface Props {
    prefs: Preferences | null;
    /** Pins and unpins, so the board is where you arrange the chin. */
    onpin: (id: string, pinned: boolean) => void;
  }

  let { prefs, onpin }: Props = $props();

  const pinned = $derived(new Set(prefs?.widgets.pinned ?? []));
</script>

<!--
  A board of tiles, and the tile chrome lives here rather than in each widget.
  A widget that draws its own frame is a widget that drifts from the others the
  first time one of them is touched, and this way a new one inherits the look
  by being put on the board.
-->
<div class="board">
  {#each WIDGETS as widget (widget.id)}
    <div class="tile" style:grid-column={`span ${widget.wide ? 2 : 1}`}>
      <div class="body">
        {#if widget.id === "clock"}
          <Clock seconds={prefs?.widgets.seconds ?? false} />
        {:else if widget.id === "weather"}
          <Weather />
        {:else if widget.id === "machine"}
          <Machine />
        {/if}
      </div>

      <!--
        The pin is the only thing on a tile you can press, and it stays quiet
        until the tile is hovered or the widget is already pinned. A board of
        six tiles each wearing a button is a toolbar, not a dashboard.
      -->
      <button
        class="pin"
        class:on={pinned.has(widget.id)}
        use:hint={pinned.has(widget.id) ? "Unpin from the launcher" : "Pin to the launcher"}
        aria-label={pinned.has(widget.id) ? `Unpin ${widget.name}` : `Pin ${widget.name}`}
        aria-pressed={pinned.has(widget.id)}
        tabindex="-1"
        onmousedown={(e) => e.preventDefault()}
        onclick={() => onpin(widget.id, !pinned.has(widget.id))}
      >
        <svg viewBox="0 0 16 16" width="13" height="13" aria-hidden="true">
          <path
            d="M6 1.5h4l-.5 4L12 8v1.5H8.75V14L8 15l-.75-1V9.5H4V8l2.5-2.5z"
            fill="currentColor"
          />
        </svg>
      </button>
    </div>
  {/each}
</div>

<style>
  /*
   * Scrolls, and that is not a detail.
   *
   * The launcher is a fixed 500px window: a board that is taller than the
   * space left between the field and the chin does not push them apart, it
   * runs underneath them. The first version did exactly that and the last row
   * of programs was drawn behind the keys.
   */
  .board {
    display: grid;
    align-content: start;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: var(--space-3);
    width: 100%;
    min-height: 0;
    padding: var(--space-3);
    overflow-y: auto;
    scrollbar-width: thin;
    scrollbar-color: var(--scrollbar-thumb) transparent;
  }

  /*
   * The tile.
   *
   * The bevel and the sheen are what make these read as objects lying on the
   * glass rather than rectangles painted on it, and they are the same two the
   * rest of Sill's tiles use, so a widget looks like it belongs here without
   * anybody choosing colours for it.
   */
  .tile {
    position: relative;
    display: flex;
    min-height: 132px;
    border-radius: var(--radius-lg);
    background-color: var(--fill-1);
    background-image: var(--sheen);
    box-shadow: var(--bevel-tile);
    transition: background-color var(--motion-enter) ease;
  }

  .tile:hover {
    background-color: var(--fill-2);
  }

  .body {
    flex: 1;
    min-width: 0;
  }

  .pin {
    position: absolute;
    top: var(--space-2);
    right: var(--space-2);
    display: grid;
    place-items: center;
    width: var(--icon-tile);
    height: var(--icon-tile);
    border: 0;
    border-radius: var(--radius-pill);
    background: transparent;
    color: var(--text-4);
    cursor: pointer;
    opacity: 0;
    transition:
      opacity var(--motion-enter) ease,
      color var(--motion-enter) ease,
      background-color var(--motion-enter) ease;
  }

  .tile:hover .pin,
  .pin.on {
    opacity: 1;
  }

  .pin:hover {
    background: var(--fill-3);
    color: var(--text-1);
  }

  /* Pinned is a state, so it takes the accent: the one colour in Sill that
     means "this one is chosen". */
  .pin.on {
    color: var(--accent);
  }
</style>

<script lang="ts">
  import { openSettings, quitApp } from "$lib/settings";
  import { popover } from "$lib/motion";
  import { itemId } from "$lib/results";

  interface Props {
    /** Runs a Sill built-in by id, e.g. "reload". */
    onbuiltin: (id: string) => void;
  }

  let { onbuiltin }: Props = $props();

  let open = $state(false);
  let selected = $state(0);
  let menu = $state<HTMLDivElement | null>(null);

  /** The menu, for `aria-controls` and for the item ids below. */
  const MENU = "sill-launcher-menu";

  /**
   * Whatever had focus when the menu opened, so it can have it back.
   *
   * Plain rather than `$state`: nothing renders from it, and making it
   * reactive would re-run the effect that sets it.
   */
  let came: HTMLElement | null = null;

  /*
   * The menu takes focus while it is open, and gives it back when it closes.
   *
   * Not for the keys: those are caught on the window, which is why the arrows
   * work here at all. It is that `aria-activedescendant` is read only from the
   * element that has focus, and with focus left in the search field a screen
   * reader announced the field while somebody arrowed through six menu items
   * it knew nothing about.
   *
   * Giving it back is the half that matters. The search field is where every
   * keystroke in this window is supposed to land, and a menu that quietly
   * keeps focus after it closes is how typing stops working with no visible
   * cause.
   */
  $effect(() => {
    if (open) {
      const had = document.activeElement;

      // Anything inside the menu is this effect seeing its own work. Taking
      // that as where focus came from would send it back to an element that
      // is about to be removed, which is the same as sending it nowhere.
      if (had instanceof HTMLElement && !menu?.contains(had)) came = had;

      menu?.focus();
      return;
    }

    came?.focus();
    came = null;
  });

  /**
   * A menu item.
   *
   * `glyph` names one of the line drawings below rather than a `SettingsIcon`.
   * The panel icons are colour plaques, and six of them stacked in a popover
   * this small would be the loudest thing on screen; a menu wants quiet
   * monochrome marks that read at a glance and inherit `currentColor`, so the
   * destructive row tints its own.
   */
  type Glyph = "gear" | "palette" | "puzzle" | "refresh" | "info" | "power";

  interface Item {
    label: string;
    glyph: Glyph;
    hint?: string;
    /** Draws a separator above this item. */
    breaks?: boolean;
    /** The one item that cannot be taken back. */
    danger?: boolean;
    run: () => void;
  }

  const items: Item[] = [
    { label: "Settings", glyph: "gear", hint: "Ctrl ,", run: () => void openSettings() },
    { label: "Appearance", glyph: "palette", run: () => void openSettings("appearance") },
    { label: "Extensions", glyph: "puzzle", run: () => void openSettings("extensions") },
    { label: "Reload Index", glyph: "refresh", breaks: true, run: () => onbuiltin("reload") },
    { label: "About Sill", glyph: "info", run: () => void openSettings("about") },
    { label: "Quit Sill", glyph: "power", breaks: true, danger: true, run: () => void quitApp() },
  ];

  function choose(index: number) {
    open = false;
    items[index]?.run();
  }

  /**
   * The menu owns the keyboard while it is open.
   *
   * Registered on the window rather than the button so arrows work without
   * having to keep focus inside a popover that is only a few rows tall.
   */
  function onKeydown(event: KeyboardEvent) {
    if (!open) return;

    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      open = false;
    } else if (event.key === "ArrowDown") {
      event.preventDefault();
      event.stopPropagation();
      selected = (selected + 1) % items.length;
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      event.stopPropagation();
      selected = (selected - 1 + items.length) % items.length;
    } else if (event.key === "Enter") {
      event.preventDefault();
      event.stopPropagation();
      choose(selected);
    }
  }
</script>

<svelte:window onkeydowncapture={onKeydown} />

{#if open}
  <!-- Click-away closes, which is why the scrim covers the whole window. -->
  <div class="scrim" role="presentation" onclick={() => (open = false)}></div>

  <!-- Grows out of the trigger it sits above, and leaves faster than it
       arrives. See $lib/motion.ts. -->
  <div
    id={MENU}
    bind:this={menu}
    class="menu sill-menu"
    role="menu"
    tabindex="-1"
    aria-label="Sill menu"
    aria-activedescendant={itemId(MENU, selected)}
    in:popover={{ origin: "bottom left" }}
    out:popover={{ origin: "bottom left", out: true }}
  >
    {#each items as item, index (item.label)}
      {#if item.breaks}
        <div class="rule" role="separator"></div>
      {/if}
      <div
        id={itemId(MENU, index)}
        class="item"
        class:selected={index === selected}
        class:danger={item.danger}
        role="menuitem"
        tabindex="-1"
        onmousemove={() => (selected = index)}
        onclick={(e) => {
          e.stopPropagation();
          choose(index);
        }}
        onkeydown={(e) => e.key === "Enter" && choose(index)}
      >
        <!--
          Phosphor Icons, regular weight (MIT, phosphoricons.com), vendored as
          path data rather than taken as a dependency: six paths is 4KB, and a
          package would pull a build step and a tree-shaking question for
          drawings that change about never.

          Filled outlines rather than the strokes these replaced, so the box is
          `fill` and carries no stroke settings. Regular weight because its
          effective stroke at 14px is 0.88px against the 0.99px this had, where
          bold would have been half again as heavy as everything around it.
        -->
        <span class="mark">
        <svg
          class="glyph"
          width="14"
          height="14"
          viewBox="0 0 256 256"
          fill="currentColor"
          aria-hidden="true"
        >
          {#if item.glyph === "gear"}
            <path d="M128,80a48,48,0,1,0,48,48A48.05,48.05,0,0,0,128,80Zm0,80a32,32,0,1,1,32-32A32,32,0,0,1,128,160Zm88-29.84q.06-2.16,0-4.32l14.92-18.64a8,8,0,0,0,1.48-7.06,107.21,107.21,0,0,0-10.88-26.25,8,8,0,0,0-6-3.93l-23.72-2.64q-1.48-1.56-3-3L186,40.54a8,8,0,0,0-3.94-6,107.71,107.71,0,0,0-26.25-10.87,8,8,0,0,0-7.06,1.49L130.16,40Q128,40,125.84,40L107.2,25.11a8,8,0,0,0-7.06-1.48A107.6,107.6,0,0,0,73.89,34.51a8,8,0,0,0-3.93,6L67.32,64.27q-1.56,1.49-3,3L40.54,70a8,8,0,0,0-6,3.94,107.71,107.71,0,0,0-10.87,26.25,8,8,0,0,0,1.49,7.06L40,125.84Q40,128,40,130.16L25.11,148.8a8,8,0,0,0-1.48,7.06,107.21,107.21,0,0,0,10.88,26.25,8,8,0,0,0,6,3.93l23.72,2.64q1.49,1.56,3,3L70,215.46a8,8,0,0,0,3.94,6,107.71,107.71,0,0,0,26.25,10.87,8,8,0,0,0,7.06-1.49L125.84,216q2.16.06,4.32,0l18.64,14.92a8,8,0,0,0,7.06,1.48,107.21,107.21,0,0,0,26.25-10.88,8,8,0,0,0,3.93-6l2.64-23.72q1.56-1.48,3-3L215.46,186a8,8,0,0,0,6-3.94,107.71,107.71,0,0,0,10.87-26.25,8,8,0,0,0-1.49-7.06Zm-16.1-6.5a73.93,73.93,0,0,1,0,8.68,8,8,0,0,0,1.74,5.48l14.19,17.73a91.57,91.57,0,0,1-6.23,15L187,173.11a8,8,0,0,0-5.1,2.64,74.11,74.11,0,0,1-6.14,6.14,8,8,0,0,0-2.64,5.1l-2.51,22.58a91.32,91.32,0,0,1-15,6.23l-17.74-14.19a8,8,0,0,0-5-1.75h-.48a73.93,73.93,0,0,1-8.68,0,8,8,0,0,0-5.48,1.74L100.45,215.8a91.57,91.57,0,0,1-15-6.23L82.89,187a8,8,0,0,0-2.64-5.1,74.11,74.11,0,0,1-6.14-6.14,8,8,0,0,0-5.1-2.64L46.43,170.6a91.32,91.32,0,0,1-6.23-15l14.19-17.74a8,8,0,0,0,1.74-5.48,73.93,73.93,0,0,1,0-8.68,8,8,0,0,0-1.74-5.48L40.2,100.45a91.57,91.57,0,0,1,6.23-15L69,82.89a8,8,0,0,0,5.1-2.64,74.11,74.11,0,0,1,6.14-6.14A8,8,0,0,0,82.89,69L85.4,46.43a91.32,91.32,0,0,1,15-6.23l17.74,14.19a8,8,0,0,0,5.48,1.74,73.93,73.93,0,0,1,8.68,0,8,8,0,0,0,5.48-1.74L155.55,40.2a91.57,91.57,0,0,1,15,6.23L173.11,69a8,8,0,0,0,2.64,5.1,74.11,74.11,0,0,1,6.14,6.14,8,8,0,0,0,5.1,2.64l22.58,2.51a91.32,91.32,0,0,1,6.23,15l-14.19,17.74A8,8,0,0,0,199.87,123.66Z" />
          {:else if item.glyph === "palette"}
            <path d="M200.77,53.89A103.27,103.27,0,0,0,128,24h-1.07A104,104,0,0,0,24,128c0,43,26.58,79.06,69.36,94.17A32,32,0,0,0,136,192a16,16,0,0,1,16-16h46.21a31.81,31.81,0,0,0,31.2-24.88,104.43,104.43,0,0,0,2.59-24A103.28,103.28,0,0,0,200.77,53.89Zm13,93.71A15.89,15.89,0,0,1,198.21,160H152a32,32,0,0,0-32,32,16,16,0,0,1-21.31,15.07C62.49,194.3,40,164,40,128a88,88,0,0,1,87.09-88h.9a88.35,88.35,0,0,1,88,87.25A88.86,88.86,0,0,1,213.81,147.6ZM140,76a12,12,0,1,1-12-12A12,12,0,0,1,140,76ZM96,100A12,12,0,1,1,84,88,12,12,0,0,1,96,100Zm0,56a12,12,0,1,1-12-12A12,12,0,0,1,96,156Zm88-56a12,12,0,1,1-12-12A12,12,0,0,1,184,100Z" />
          {:else if item.glyph === "puzzle"}
            <path d="M220.27,158.54a8,8,0,0,0-7.7-.46,20,20,0,1,1,0-36.16A8,8,0,0,0,224,114.69V72a16,16,0,0,0-16-16H171.78a35.36,35.36,0,0,0,.22-4,36.11,36.11,0,0,0-11.36-26.24,36,36,0,0,0-60.55,23.62,36.56,36.56,0,0,0,.14,6.62H64A16,16,0,0,0,48,72v32.22a35.36,35.36,0,0,0-4-.22,36.12,36.12,0,0,0-26.24,11.36,35.7,35.7,0,0,0-9.69,27,36.08,36.08,0,0,0,33.31,33.6,35.68,35.68,0,0,0,6.62-.14V208a16,16,0,0,0,16,16H208a16,16,0,0,0,16-16V165.31A8,8,0,0,0,220.27,158.54ZM208,208H64V165.31a8,8,0,0,0-11.43-7.23,20,20,0,1,1,0-36.16A8,8,0,0,0,64,114.69V72h46.69a8,8,0,0,0,7.23-11.43,20,20,0,1,1,36.16,0A8,8,0,0,0,161.31,72H208v32.23a35.68,35.68,0,0,0-6.62-.14A36,36,0,0,0,204,176a35.36,35.36,0,0,0,4-.22Z" />
          {:else if item.glyph === "refresh"}
            <path d="M224,48V96a8,8,0,0,1-8,8H168a8,8,0,0,1,0-16h28.69L182.06,73.37a79.56,79.56,0,0,0-56.13-23.43h-.45A79.52,79.52,0,0,0,69.59,72.71,8,8,0,0,1,58.41,61.27a96,96,0,0,1,135,.79L208,76.69V48a8,8,0,0,1,16,0ZM186.41,183.29a80,80,0,0,1-112.47-.66L59.31,168H88a8,8,0,0,0,0-16H40a8,8,0,0,0-8,8v48a8,8,0,0,0,16,0V179.31l14.63,14.63A95.43,95.43,0,0,0,130,222.06h.53a95.36,95.36,0,0,0,67.07-27.33,8,8,0,0,0-11.18-11.44Z" />
          {:else if item.glyph === "info"}
            <path d="M128,24A104,104,0,1,0,232,128,104.11,104.11,0,0,0,128,24Zm0,192a88,88,0,1,1,88-88A88.1,88.1,0,0,1,128,216Zm16-40a8,8,0,0,1-8,8,16,16,0,0,1-16-16V128a8,8,0,0,1,0-16,16,16,0,0,1,16,16v40A8,8,0,0,1,144,176ZM112,84a12,12,0,1,1,12,12A12,12,0,0,1,112,84Z" />
          {:else}
            <!-- Power: the one glyph nobody has to be taught. -->
            <path d="M120,128V48a8,8,0,0,1,16,0v80a8,8,0,0,1-16,0Zm60.37-78.7a8,8,0,0,0-8.74,13.4C194.74,77.77,208,101.57,208,128a80,80,0,0,1-160,0c0-26.43,13.26-50.23,36.37-65.3a8,8,0,0,0-8.74-13.4C47.9,67.38,32,96.06,32,128a96,96,0,0,0,192,0C224,96.06,208.1,67.38,180.37,49.3Z" />
          {/if}
        </svg>
        </span>
        <span class="label">{item.label}</span>
        {#if item.hint}
          <span class="spacer"></span>
          <!--
            One cap per key, split on the space the hint is written with.
            The same `.sill-key` the chin and the action panel draw, so the
            three surfaces that show a chord show it one way.
          -->
          <span class="hint">
            {#each item.hint.split(" ") as key, k (k)}
              <span class="sill-key">{key}</span>
            {/each}
          </span>
        {/if}
      </div>
    {/each}
  </div>
{/if}

<!--
  Sill's mark, on the button that opens Sill's menu.

  The mark stood here once beside a caret and was taken out on the grounds that
  a corner button is not the place for an identity. This is now the only place
  for one. The top of the window has given its lead slot to whichever face is
  on screen, so the picture that says which application this is cannot live up
  there any more, and the application's own menu is the right thing to hang it
  on: pressing the mark opens the things that belong to Sill rather than to
  what Sill is showing.

  Still a real button rather than a bare glyph: it has a target and a hover
  fill, and wears that same fill while the menu is open. The caret said whether
  the menu was open by turning over; the open menu, an inch above it, says the
  same thing louder.

  `tabindex="-1"` with mousedown prevented, because the search field must keep
  document focus. A plain button takes it on click and the arrow keys then stop
  moving the selection with no visible cause.
-->
<button
  class="trigger"
  class:open
  tabindex="-1"
  aria-label="Sill menu"
  aria-haspopup="menu"
  aria-expanded={open}
  aria-controls={open ? MENU : undefined}
  onmousedown={(e) => e.preventDefault()}
  onclick={(e) => {
    e.stopPropagation();
    open = !open;
    selected = 0;
  }}
>
  <img class="mark" src="/sill.png" alt="" width="20" height="20" draggable="false" />
</button>

<style>
  .trigger {
    display: grid;
    place-items: center;
    flex: none;
    height: var(--control-height);
    padding: 0 var(--space-2);
    border: 0;
    border-radius: var(--radius-lg);
    background: transparent;
    font: inherit;
    cursor: default;
    transition: background-color var(--motion-state) var(--ease);
  }

  /*
   * A step under the tile a list row draws an icon in.
   *
   * The chin is a 40px band carrying 30px controls. A 26px tile inside one of
   * those leaves two pixels above and below it, which is the mark standing
   * flush with the edge of its own hover fill rather than on it.
   */
  .mark {
    flex: none;
    width: var(--icon-tile-sm);
    height: var(--icon-tile-sm);
    -webkit-user-drag: none;
  }

  .trigger:hover,
  .trigger.open {
    background-color: var(--fill-2);
  }

  .scrim {
    position: fixed;
    inset: 0;
    z-index: var(--z-menu-scrim);
  }

  /* Rises out of its own button, so it is anchored to the bottom left.
     `bottom` clears the chin by one step, from the token Rust sizes the
     window with. Capped so that at four visible rows, the smallest window
     there is, the menu scrolls rather than covering the search field. */
  .menu {
    position: fixed;
    /*
     * Chromium's own focus ring, off.
     *
     * This element is focusable only so the arrow keys have somewhere to
     * land. Chromium paints `outline: auto` around whatever holds focus
     * once the last input was a keyboard, which in a launcher it always
     * just was, and on a dark popover that ring is drawn white and traces
     * the whole menu. It goes the instant focus leaves, so it reads as the
     * menu flickering a border rather than as a focus indicator.
     *
     * Nothing is lost. Which row the keys are on is drawn by `.selected`
     * and announced through `aria-activedescendant`, and neither of those
     * is this element. Forced colours put a real outline back, which is
     * exactly what that block in `theme.css` is for.
     */
    outline: none;
    left: var(--space-2);
    bottom: calc(var(--chin-height) + var(--space-1));
    z-index: var(--z-menu);
    width: 400px;
    max-height: calc(100vh - var(--chin-height) - var(--space-8));
    overflow-y: auto;
    padding: var(--space-1);
  }

  /*
   * The same object the action panel draws, because they are the same kind
   * of thing: a list of what you can do, one press from the chin.
   *
   * Matched rather than merely similar. Thirty pixels against thirty-two,
   * and eight of padding against twelve, are each too small to look wrong
   * alone; together they are why one menu read as a different control from
   * the other.
   */
  .item {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    height: 32px;
    padding: 0 var(--space-2);
    border-radius: var(--radius-md);
    font-size: var(--text-body);
    font-weight: var(--weight-body);
    color: var(--text-2);
    cursor: default;
    transition:
      background-color var(--motion-state) var(--ease),
      color var(--motion-state) var(--ease);
  }

  /*
   * The mark column, sized like the panel's rather than left as whatever
   * the drawing happens to be.
   *
   * Fixed and always present, so labels line up whether a glyph is wide or
   * narrow. A column that fits itself to each drawing is a ragged left
   * edge, which is the thing an icon column exists to fix.
   */
  .mark {
    display: grid;
    place-items: center;
    flex: none;
    width: var(--icon-tile-xs);
    height: var(--icon-tile-xs);
    color: var(--text-2);
    transition: color var(--motion-state) var(--ease);
  }

  .glyph {
    flex: none;
  }

  .item.selected {
    background-color: var(--accent-fill);
    color: var(--text-1);
  }

  .item.selected .mark {
    color: var(--text-2);
  }

  /* The one row that cannot be taken back says so in the one colour that
     means it, and only once it is under the cursor. Red on every render
     would make quitting the loudest thing in the menu. */
  .item.danger.selected,
  .item.danger.selected .mark {
    color: var(--danger);
  }

  .rule {
    height: 1px;
    margin: var(--space-1) var(--space-2);
    background: var(--hairline);
  }

  .spacer {
    flex: 1;
  }

  /* Caps rather than grey words. The note that used to be here said a
     monospace key name beside a proportional one puts two typefaces in a
     four-row menu, which is still true: `.sill-key` inherits the face and
     sets only a size and a ground, so this stays one typeface. */
  .hint {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    flex: none;
  }
</style>

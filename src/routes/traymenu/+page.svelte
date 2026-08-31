<script lang="ts">
  /**
   * The notification-area menu.
   *
   * A window rather than a `tauri::menu::Menu`, because a native Windows menu
   * is drawn by the shell and takes none of Sill's design: no glass, no
   * keycaps, no glyphs, the system font at the system size. This is the one
   * surface a user meets without opening the launcher, so it looking like a
   * 1995 context menu says more about the app than it should.
   *
   * The window is transparent and borderless; Rust positions it at the cursor
   * and shows it. It hides on blur, on Escape, and after anything is chosen.
   */
  import { onMount } from "svelte";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { applyAppearance, getPreferences, openSettings, quitApp } from "$lib/settings";
  import { summonWith } from "$lib/exthost/commands";
  import "$lib/theme/theme.css";

  const win = getCurrentWindow();

  type Glyph = "search" | "clipboard" | "scissors" | "mic" | "gear" | "power";

  interface Item {
    label: string;
    glyph: Glyph;
    hint?: string;
    /** Draws a separator above this item. */
    breaks?: boolean;
    danger?: boolean;
    run: () => Promise<void> | void;
  }

  /** Set from Rust, so the row shows the hotkey actually bound. */
  let summonHotkey = $state("");
  let selected = $state(0);

  /**
   * Everything here goes through `summonWith`, which is one intent: put the
   * launcher up, and run this when it arrives. The tray menu never loads an
   * extension itself, because a command rendering into a 216px popover that is
   * about to hide is not what anybody asked for.
   */
  const items: Item[] = [
    { label: "Open Sill", glyph: "search", run: () => summonWith() },
    { label: "Clipboard History", glyph: "clipboard", run: () => summonWith("sill:clipboard") },
    { label: "Snippets", glyph: "scissors", run: () => summonWith("sill:snippets") },
    { label: "Dictate", glyph: "mic", run: () => summonWith("sill:dictate") },
    { label: "Settings", glyph: "gear", hint: "Ctrl ,", breaks: true, run: () => openSettings() },
    { label: "Quit Sill", glyph: "power", breaks: true, danger: true, run: () => quitApp() },
  ];

  async function choose(index: number) {
    const item = items[index];
    if (!item) return;

    /*
     * Hidden first, but never at the cost of the action.
     *
     * `hide` is a core plugin call, so it is one of the things a window that
     * is missing from `capabilities/default.json` has denied. Awaiting it
     * unguarded meant a rejected hide threw before `run` was ever reached, and
     * the whole menu read as dead: it drew perfectly and no click did
     * anything. Whatever happens to the window, the thing that was clicked
     * still runs. `tests/acl_parity.rs` stops the underlying cause recurring.
     */
    try {
      await win.hide();
    } catch (err) {
      console.error("[sill] tray menu could not hide", err);
    }

    try {
      await item.run();
    } catch (err) {
      console.error("[sill] tray menu action failed", err);
    }
  }

  function onKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      void win.hide();
    } else if (event.key === "ArrowDown") {
      event.preventDefault();
      selected = (selected + 1) % items.length;
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      selected = (selected - 1 + items.length) % items.length;
    } else if (event.key === "Enter") {
      event.preventDefault();
      void choose(selected);
    }
  }

  /**
   * The hotkey actually bound, and the theme and face this window should wear.
   *
   * This window never applied appearance at all, so it kept the stylesheet's
   * defaults while every other window followed the preference. Nobody noticed
   * while the only setting was the font, because it is six words of UI; a
   * palette would have made it obvious the moment a theme was picked.
   */
  async function readAppearance() {
    try {
      const prefs = await getPreferences();
      applyAppearance(prefs);
      summonHotkey = prefs.hotkey.summon.replaceAll("+", " ");
    } catch {
      // A menu that will not open because it could not read a hint is worse
      // than one that shows the row without it.
      summonHotkey = "";
    }
  }

  onMount(() => {
    const stops: UnlistenFn[] = [];
    void readAppearance();

    // Every showing starts at the top. A menu that remembers where the cursor
    // was last time highlights a row nobody is looking at. The hotkey is
    // re-read too, since it can have been rebound since the last showing.
    void listen("sill://tray-menu-shown", () => {
      selected = 0;
      void readAppearance();
    }).then((stop) => stops.push(stop));

    return () => stops.forEach((stop) => stop());
  });
</script>

<svelte:window onkeydown={onKeydown} onblur={() => void win.hide()} />

<div class="menu" role="menu" tabindex="-1">
  {#each items as item, index (item.label)}
    {#if item.breaks}
      <div class="rule" role="separator"></div>
    {/if}
    <div
      class="item"
      class:selected={index === selected}
      class:danger={item.danger}
      role="menuitem"
      tabindex="-1"
      onmousemove={() => (selected = index)}
      onclick={() => void choose(index)}
      onkeydown={(e) => e.key === "Enter" && void choose(index)}
    >
      <!--
        Phosphor Icons, regular weight (MIT, phosphoricons.com), the same set
        and the same weight the launcher's own menu uses, so the two menus are
        one design rather than two hands.
      -->
      <svg
        class="glyph"
        width="14"
        height="14"
        viewBox="0 0 256 256"
        fill="currentColor"
        aria-hidden="true"
      >
        {#if item.glyph === "search"}
          <path d="M229.66,218.34l-50.07-50.06a88.11,88.11,0,1,0-11.31,11.31l50.06,50.07a8,8,0,0,0,11.32-11.32ZM40,112a72,72,0,1,1,72,72A72.08,72.08,0,0,1,40,112Z" />
        {:else if item.glyph === "clipboard"}
          <path d="M168,152a8,8,0,0,1-8,8H96a8,8,0,0,1,0-16h64A8,8,0,0,1,168,152Zm-8-40H96a8,8,0,0,0,0,16h64a8,8,0,0,0,0-16Zm56-64V216a16,16,0,0,1-16,16H56a16,16,0,0,1-16-16V48A16,16,0,0,1,56,32H92.26a47.92,47.92,0,0,1,71.48,0H200A16,16,0,0,1,216,48ZM96,64h64a32,32,0,0,0-64,0ZM200,48H173.25A47.93,47.93,0,0,1,176,64v8a8,8,0,0,1-8,8H88a8,8,0,0,1-8-8V64a47.93,47.93,0,0,1,2.75-16H56V216H200Z" />
        {:else if item.glyph === "scissors"}
          <path d="M157.73,113.13A8,8,0,0,1,159.82,102L227.48,55.7a8,8,0,0,1,9,13.21l-67.67,46.3a7.92,7.92,0,0,1-4.51,1.4A8,8,0,0,1,157.73,113.13Zm80.87,85.09a8,8,0,0,1-11.12,2.08L136,137.7,93.49,166.78a36,36,0,1,1-9-13.19L121.83,128,84.44,102.41a35.86,35.86,0,1,1,9-13.19l143,97.87A8,8,0,0,1,238.6,198.22ZM80,180a20,20,0,1,0-5.86,14.14A19.85,19.85,0,0,0,80,180ZM74.14,90.13a20,20,0,1,0-28.28,0A19.85,19.85,0,0,0,74.14,90.13Z" />
        {:else if item.glyph === "mic"}
          <path d="M128,176a48.05,48.05,0,0,0,48-48V64a48,48,0,0,0-96,0v64A48.05,48.05,0,0,0,128,176ZM96,64a32,32,0,0,1,64,0v64a32,32,0,0,1-64,0Zm40,143.6V240a8,8,0,0,1-16,0V207.6A80.11,80.11,0,0,1,48,128a8,8,0,0,1,16,0,64,64,0,0,0,128,0,8,8,0,0,1,16,0A80.11,80.11,0,0,1,136,207.6Z" />
        {:else if item.glyph === "gear"}
          <path d="M128,80a48,48,0,1,0,48,48A48.05,48.05,0,0,0,128,80Zm0,80a32,32,0,1,1,32-32A32,32,0,0,1,128,160Zm88-29.84q.06-2.16,0-4.32l14.92-18.64a8,8,0,0,0,1.48-7.06,107.21,107.21,0,0,0-10.88-26.25,8,8,0,0,0-6-3.93l-23.72-2.64q-1.48-1.56-3-3L186,40.54a8,8,0,0,0-3.94-6,107.71,107.71,0,0,0-26.25-10.87,8,8,0,0,0-7.06,1.49L130.16,40Q128,40,125.84,40L107.2,25.11a8,8,0,0,0-7.06-1.48A107.6,107.6,0,0,0,73.89,34.51a8,8,0,0,0-3.93,6L67.32,64.27q-1.56,1.49-3,3L40.54,70a8,8,0,0,0-6,3.94,107.71,107.71,0,0,0-10.87,26.25,8,8,0,0,0,1.49,7.06L40,125.84Q40,128,40,130.16L25.11,148.8a8,8,0,0,0-1.48,7.06,107.21,107.21,0,0,0,10.88,26.25,8,8,0,0,0,6,3.93l23.72,2.64q1.49,1.56,3,3L70,215.46a8,8,0,0,0,3.94,6,107.71,107.71,0,0,0,26.25,10.87,8,8,0,0,0,7.06-1.49L125.84,216q2.16.06,4.32,0l18.64,14.92a8,8,0,0,0,7.06,1.48,107.21,107.21,0,0,0,26.25-10.88,8,8,0,0,0,3.93-6l2.64-23.72q1.56-1.48,3-3L215.46,186a8,8,0,0,0,6-3.94,107.71,107.71,0,0,0,10.87-26.25,8,8,0,0,0-1.49-7.06Zm-16.1-6.5a73.93,73.93,0,0,1,0,8.68,8,8,0,0,0,1.74,5.48l14.19,17.73a91.57,91.57,0,0,1-6.23,15L187,173.11a8,8,0,0,0-5.1,2.64,74.11,74.11,0,0,1-6.14,6.14,8,8,0,0,0-2.64,5.1l-2.51,22.58a91.32,91.32,0,0,1-15,6.23l-17.74-14.19a8,8,0,0,0-5-1.75h-.48a73.93,73.93,0,0,1-8.68,0,8,8,0,0,0-5.48,1.74L100.45,215.8a91.57,91.57,0,0,1-15-6.23L82.89,187a8,8,0,0,0-2.64-5.1,74.11,74.11,0,0,1-6.14-6.14,8,8,0,0,0-5.1-2.64L46.43,170.6a91.32,91.32,0,0,1-6.23-15l14.19-17.74a8,8,0,0,0,1.74-5.48,73.93,73.93,0,0,1,0-8.68,8,8,0,0,0-1.74-5.48L40.2,100.45a91.57,91.57,0,0,1,6.23-15L69,82.89a8,8,0,0,0,5.1-2.64,74.11,74.11,0,0,1,6.14-6.14A8,8,0,0,0,82.89,69L85.4,46.43a91.32,91.32,0,0,1,15-6.23l17.74,14.19a8,8,0,0,0,5.48,1.74,73.93,73.93,0,0,1,8.68,0,8,8,0,0,0,5.48-1.74L155.55,40.2a91.57,91.57,0,0,1,15,6.23L173.11,69a8,8,0,0,0,2.64,5.1,74.11,74.11,0,0,1,6.14,6.14,8,8,0,0,0,5.1,2.64l22.58,2.51a91.32,91.32,0,0,1,6.23,15l-14.19,17.74A8,8,0,0,0,199.87,123.66Z" />
        {:else}
          <path d="M120,128V48a8,8,0,0,1,16,0v80a8,8,0,0,1-16,0Zm60.37-78.7a8,8,0,0,0-8.74,13.4C194.74,77.77,208,101.57,208,128a80,80,0,0,1-160,0c0-26.43,13.26-50.23,36.37-65.3a8,8,0,0,0-8.74-13.4C47.9,67.38,32,96.06,32,128a96,96,0,0,0,192,0C224,96.06,208.1,67.38,180.37,49.3Z" />
        {/if}
      </svg>
      <span class="label">{item.label}</span>
      <span class="spacer"></span>
      {#if index === 0 && summonHotkey}
        <span class="sill-key">{summonHotkey}</span>
      {:else if item.hint}
        <span class="hint">{item.hint}</span>
      {/if}
    </div>
  {/each}
</div>

<style>
  /*
   * Built like the launcher, not like a popover.
   *
   * `.sill-menu` is deliberately not used here. Its `backdrop-filter` blurs
   * the launcher's own content behind a popover, and there is no content
   * behind THIS: it is the whole window, floating over the desktop. The filter
   * would blur nothing and the remaining alpha would be raw desktop showing
   * through, which is exactly how it looked.
   *
   * So it takes the window recipe instead: OS acrylic underneath (applied in
   * `lib.rs` alongside the launcher's), a tint on top, and the window radius
   * rather than a card radius, because DWM is clipping this window's corners
   * too. No border and no outer shadow: this IS the window, so everything
   * outward is the OS's job and an inset light catch is all that is left.
   */
  .menu {
    display: flex;
    flex-direction: column;
    height: 100vh;
    padding: var(--space-1);
    background-color: color-mix(
      in srgb,
      var(--core-secondary-background) calc((1 - var(--glass-strength)) * 100%),
      var(--surface-base)
    );
    background-image: var(--chroma), linear-gradient(var(--tint-menu), var(--tint-menu));
    border-radius: var(--radius-window);
    box-shadow: var(--bevel-window);
    overflow: hidden;
  }

  .item {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    height: 30px;
    padding: 0 var(--space-2);
    border-radius: var(--radius-md);
    font-size: var(--text-body);
    color: var(--text-2);
    cursor: default;
    transition: background-color 0.15s var(--ease), color 0.15s var(--ease);
  }

  .glyph {
    flex: none;
    color: var(--text-3);
    transition: color 0.15s var(--ease);
  }

  .item.selected {
    background-color: var(--accent-fill);
    color: var(--text-1);
  }

  .item.selected .glyph {
    color: var(--text-2);
  }

  /* Only once it is under the cursor. Red on every render would make
     quitting the loudest thing in the menu. */
  .item.danger.selected,
  .item.danger.selected .glyph {
    color: var(--accent-red);
  }

  .label {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .spacer {
    flex: 1;
    min-width: var(--space-2);
  }

  .hint {
    flex: none;
    font-size: var(--text-meta);
    color: var(--text-4);
  }

  .rule {
    height: 1px;
    margin: var(--space-1) var(--space-2);
    background: var(--hairline);
  }
</style>

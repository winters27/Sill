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
      <svg
        class="glyph"
        width="14"
        height="14"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="1.7"
        stroke-linecap="round"
        stroke-linejoin="round"
        aria-hidden="true"
      >
        {#if item.glyph === "search"}
          <circle cx="11" cy="11" r="7" />
          <path d="m20 20-3.6-3.6" />
        {:else if item.glyph === "clipboard"}
          <rect x="4.5" y="4" width="15" height="17" rx="2" />
          <path d="M9 4V3a1 1 0 0 1 1-1h4a1 1 0 0 1 1 1v1Z" />
          <path d="M8.5 11h7M8.5 15h4" />
        {:else if item.glyph === "scissors"}
          <circle cx="6" cy="6.5" r="2.6" />
          <circle cx="6" cy="17.5" r="2.6" />
          <path d="M8.3 8 20 17M20 7 8.3 16" />
        {:else if item.glyph === "mic"}
          <rect x="9" y="2.5" width="6" height="11" rx="3" />
          <path d="M5 11a7 7 0 0 0 14 0" />
          <path d="M12 18v3M8.5 21h7" />
        {:else if item.glyph === "gear"}
          <circle cx="12" cy="12" r="3.2" />
          <path d="M12 2.6v2.6M12 18.8v2.6M21.4 12h-2.6M5.2 12H2.6M18.6 5.4l-1.8 1.8M7.2 16.8l-1.8 1.8M18.6 18.6l-1.8-1.8M7.2 7.2 5.4 5.4" />
        {:else}
          <path d="M12 3v8.5" />
          <path d="M7.1 6.4a8 8 0 1 0 9.8 0" />
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

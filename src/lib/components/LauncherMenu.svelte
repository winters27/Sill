<script lang="ts">
  import { openSettings, quitApp } from "$lib/settings";
  import { popover } from "$lib/motion";

  interface Props {
    /** Runs a Sill built-in by id, e.g. "reload". */
    onbuiltin: (id: string) => void;
  }

  let { onbuiltin }: Props = $props();

  let open = $state(false);
  let selected = $state(0);

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
    class="menu sill-menu"
    role="menu"
    tabindex="-1"
    in:popover={{ origin: "bottom left" }}
    out:popover={{ origin: "bottom left", out: true }}
  >
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
        onclick={(e) => {
          e.stopPropagation();
          choose(index);
        }}
        onkeydown={(e) => e.key === "Enter" && choose(index)}
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
          {#if item.glyph === "gear"}
            <circle cx="12" cy="12" r="3.2" />
            <path d="M12 2.6v2.6M12 18.8v2.6M21.4 12h-2.6M5.2 12H2.6M18.6 5.4l-1.8 1.8M7.2 16.8l-1.8 1.8M18.6 18.6l-1.8-1.8M7.2 7.2 5.4 5.4" />
          {:else if item.glyph === "palette"}
            <circle cx="12" cy="12" r="9" />
            <path d="M12 3a9 9 0 0 0 0 18 4.5 4.5 0 0 0 0-9 4.5 4.5 0 0 1 0-9Z" />
          {:else if item.glyph === "puzzle"}
            <path d="M9 4.5a2 2 0 1 1 4 0V6h3.2a.8.8 0 0 1 .8.8V10h1.5a2 2 0 1 1 0 4H17v3.2a.8.8 0 0 1-.8.8H13v-1.5a2 2 0 1 0-4 0V18H5.8a.8.8 0 0 1-.8-.8V6.8a.8.8 0 0 1 .8-.8H9V4.5Z" />
          {:else if item.glyph === "refresh"}
            <path d="M20.5 12a8.5 8.5 0 1 1-2.6-6.1" />
            <path d="M21 4v4.5h-4.5" />
          {:else if item.glyph === "info"}
            <circle cx="12" cy="12" r="9" />
            <path d="M12 11v5.5M12 7.8h.01" />
          {:else}
            <!-- Power: the one glyph nobody has to be taught. -->
            <path d="M12 3v8.5" />
            <path d="M7.1 6.4a8 8 0 1 0 9.8 0" />
          {/if}
        </svg>
        <span class="label">{item.label}</span>
        {#if item.hint}
          <span class="spacer"></span>
          <span class="hint">{item.hint}</span>
        {/if}
      </div>
    {/each}
  </div>
{/if}

<!--
  The mark and a chevron, and no label.

  It carried the word "Sill" briefly, which said nothing: the mark already is
  Sill's identity. Naming the running extension instead would have duplicated
  the crumb the search row already shows. A label with no unique job in either
  state is not a label, so what is left is an identity and an affordance, which
  is all a menu button in a corner has to be.

  Still a real button rather than the bare glyph this replaced: it has a
  target, a hover state and a chevron saying a menu rises out of it.

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
  onmousedown={(e) => e.preventDefault()}
  onclick={(e) => {
    e.stopPropagation();
    open = !open;
    selected = 0;
  }}
>
  <img src="/sill.png" alt="" width="18" height="18" draggable="false" />
  <svg class="chevron" width="9" height="9" viewBox="0 0 12 12" aria-hidden="true">
    <path d="M2.5 7.5 6 4l3.5 3.5" stroke="currentColor" stroke-width="1.6"
      stroke-linecap="round" stroke-linejoin="round" fill="none" />
  </svg>
</button>

<style>
  .trigger {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    flex: none;
    height: 30px;
    padding: 0 var(--space-2);
    border: 0;
    border-radius: var(--radius-lg);
    background: transparent;
    color: var(--text-2);
    font: inherit;
    cursor: default;
    transition: background-color 0.16s var(--ease), color 0.16s var(--ease);
  }

  .trigger img {
    flex: none;
    -webkit-user-drag: none;
  }

  /* Points up, because the menu rises. */
  .chevron {
    flex: none;
    color: var(--text-4);
    transition: color 0.16s var(--ease);
  }

  .trigger:hover,
  .trigger.open {
    background-color: var(--fill-2);
    color: var(--text-1);
  }

  .trigger:hover .chevron,
  .trigger.open .chevron {
    color: var(--text-2);
  }

  .scrim {
    position: fixed;
    inset: 0;
    z-index: 20;
  }

  /* Rises out of its own button, so it is anchored to the bottom left.
     `bottom` clears the 40px footer. */
  .menu {
    position: fixed;
    left: var(--space-2);
    bottom: 44px;
    z-index: 21;
    width: 208px;
    padding: var(--space-1);
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

  /* The one row that cannot be taken back says so in the one colour that
     means it, and only once it is under the cursor. Red on every render
     would make quitting the loudest thing in the menu. */
  .item.danger.selected,
  .item.danger.selected .glyph {
    color: var(--accent-red);
  }

  .rule {
    height: 1px;
    margin: var(--space-1) var(--space-2);
    background: var(--hairline);
  }

  .spacer {
    flex: 1;
  }

  /* The same face as the label. A monospace key name beside a proportional
     one puts two typefaces in a four-row menu. */
  .hint {
    font-size: var(--text-meta);
    color: var(--text-3);
  }
</style>

<script lang="ts">
  import { openSettings, quitApp } from "$lib/settings";

  interface Props {
    /** Runs a Sill built-in by id, e.g. "reload". */
    onbuiltin: (id: string) => void;
  }

  let { onbuiltin }: Props = $props();

  let open = $state(false);
  let selected = $state(0);

  interface Item {
    label: string;
    hint?: string;
    run: () => void;
  }

  const items: Item[] = [
    { label: "Settings", hint: "Ctrl ,", run: () => void openSettings() },
    { label: "Reload Index", run: () => onbuiltin("reload") },
    { label: "About Sill", run: () => void openSettings("about") },
    { label: "Quit Sill", run: () => void quitApp() },
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

  <div class="menu sill-menu" role="menu" tabindex="-1">
    {#each items as item, index (item.label)}
      <div
        class="item"
        class:selected={index === selected}
        role="menuitem"
        tabindex="-1"
        onmousemove={() => (selected = index)}
        onclick={(e) => {
          e.stopPropagation();
          choose(index);
        }}
        onkeydown={(e) => e.key === "Enter" && choose(index)}
      >
        <span class="label">{item.label}</span>
        {#if item.hint}
          <span class="spacer"></span>
          <span class="hint">{item.hint}</span>
        {/if}
      </div>
    {/each}
  </div>
{/if}

<button
  class="trigger"
  class:open
  aria-label="Sill menu"
  aria-haspopup="menu"
  aria-expanded={open}
  onclick={(e) => {
    e.stopPropagation();
    open = !open;
    selected = 0;
  }}
>
  <svg width="14" height="14" viewBox="0 0 16 16" aria-hidden="true">
    <path d="M2 4.5h12M2 8h12M2 11.5h12" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
  </svg>
</button>

<style>
  .trigger {
    display: grid;
    place-items: center;
    width: 24px;
    height: 22px;
    padding: 0;
    margin-left: -6px;
    border: 0;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--text-muted);
    font: inherit;
    font-size: 12px;
    cursor: pointer;
    transition: background-color 0.16s var(--ease), color 0.16s var(--ease);
  }

  .trigger:hover,
  .trigger.open {
    background-color: rgba(var(--accent-rgb), 0.1);
    color: var(--core-foreground);
  }

  .scrim {
    position: fixed;
    inset: 0;
    z-index: 20;
  }

  /* Rises out of its own button, so it is anchored to the bottom left. */
  .menu {
    position: fixed;
    left: 8px;
    bottom: 36px;
    z-index: 21;
    width: 190px;
    padding: 5px;
  }

  .item {
    display: flex;
    align-items: center;
    height: 30px;
    padding: 0 9px;
    border-radius: var(--radius-sm);
    font-size: 13px;
    cursor: default;
    transition: background-color 0.15s var(--ease);
  }

  .item.selected {
    background-color: var(--surface);
  }

  .spacer {
    flex: 1;
  }

  .hint {
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--text-faint);
  }
</style>

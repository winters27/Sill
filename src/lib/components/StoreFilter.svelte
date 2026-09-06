<script lang="ts">
  /**
   * The store's filter, beside the search field.
   *
   * One control where three chips and sixteen more used to sit on a strip
   * under the field: what to show (everything, what is installed, what has
   * an update) and which category, each category with the mark Rust chose
   * for it, so the same picture appears here and on the rows. A native
   * `<select>` cannot draw a mark beside an option, which is why this is a
   * menu of Sill's own rather than the dropdown an extension gets.
   *
   * Nothing is decided here. Rust decides the categories and their marks and
   * the store view decides what the filter means; this draws the state it is
   * handed and reports a pick as a value the view already understands:
   * `scope:all`, `scope:installed`, `scope:updates`, `category:` for any, and
   * `category:<name>`.
   *
   * Arrows walk the menu, Escape closes it and hands focus back to the
   * trigger, the same as the clipboard's own menus.
   */
  import ExtIcon from "./ExtIcon.svelte";
  import { hint } from "$lib/hint";
  import { rovingTo } from "$lib/roving";
  import type { StoreFilterState } from "$lib/store";

  interface Props {
    filter: StoreFilterState;
    onpick: (value: string) => void;
  }

  let { filter, onpick }: Props = $props();

  let open = $state(false);
  let trigger = $state<HTMLButtonElement | null>(null);

  const SCOPES = [
    { value: "scope:all", title: "All", mark: "List" },
    { value: "scope:installed", title: "Installed", mark: "CheckCircle" },
    { value: "scope:updates", title: "Updates", mark: "ArrowClockwise" },
  ] as const;

  const scopeOf = $derived(SCOPES.find((one) => one.value === `scope:${filter.scope}`) ?? SCOPES[0]);

  /** What the trigger says: the category when one is chosen, else the scope. */
  const showing = $derived.by(() => {
    if (filter.category) {
      const found = filter.categories.find((one) => one.name === filter.category);
      return { title: filter.category, mark: found?.mark ?? "Tag" };
    }
    return { title: scopeOf.title, mark: scopeOf.mark };
  });

  function choose(value: string) {
    open = false;
    onpick(value);
    trigger?.focus();
  }

  function focusFirst(node: HTMLElement): void {
    (node.querySelector<HTMLElement>('.option[aria-checked="true"]') ??
      node.querySelector<HTMLElement>(".option") ??
      node).focus();
  }

  function menuKeys(event: KeyboardEvent): void {
    const menu = event.currentTarget as HTMLElement;
    const options = Array.from(menu.querySelectorAll<HTMLElement>(".option"));
    const at = options.indexOf(document.activeElement as HTMLElement);

    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      open = false;
      trigger?.focus();
      return;
    }

    const next = rovingTo(event.key, Math.max(at, 0), options.length, "column");
    if (next === null) return;
    event.preventDefault();
    event.stopPropagation();
    options[next]?.focus();
  }

  /** A click anywhere else closes it. */
  function away(node: HTMLElement) {
    const onDown = (event: PointerEvent) => {
      if (!node.contains(event.target as Node)) open = false;
    };
    document.addEventListener("pointerdown", onDown, true);
    return { destroy: () => document.removeEventListener("pointerdown", onDown, true) };
  }
</script>

<div class="filter" use:away>
  <button
    bind:this={trigger}
    type="button"
    class="trigger"
    class:open
    aria-haspopup="menu"
    aria-expanded={open}
    use:hint={"Show only some of the store"}
    onclick={() => (open = !open)}
  >
    <ExtIcon icon={{ kind: "mark", name: showing.mark }} small />
    <span class="label">{showing.title}</span>
    <svg class="chevron" viewBox="0 0 12 12" aria-hidden="true">
      <path d="M2.8 4.6 6 7.8l3.2-3.2" />
    </svg>
  </button>

  {#if open}
    <div class="menu sill-menu sill-scrolls" role="menu" tabindex="-1" use:focusFirst onkeydown={menuKeys}>
      <div class="group" role="presentation">Show</div>
      {#each SCOPES as scope (scope.value)}
        <button
          type="button"
          class="option"
          role="menuitemradio"
          aria-checked={!filter.category && filter.scope === scope.value.slice(6)}
          onclick={() => choose(scope.value)}
        >
          <ExtIcon icon={{ kind: "mark", name: scope.mark }} small />
          <span class="text">{scope.title}</span>
          {#if scope.value === "scope:updates" && filter.updates}
            <span class="count">{filter.updates}</span>
          {/if}
        </button>
      {/each}

      <div class="group" role="presentation">Category</div>
      <button
        type="button"
        class="option"
        role="menuitemradio"
        aria-checked={filter.category === null}
        onclick={() => choose("category:")}
      >
        <ExtIcon icon={{ kind: "mark", name: "Tag" }} small />
        <span class="text">Any</span>
      </button>
      {#each filter.categories as one (one.name)}
        <button
          type="button"
          class="option"
          role="menuitemradio"
          aria-checked={filter.category === one.name}
          onclick={() => choose(`category:${one.name}`)}
        >
          <ExtIcon icon={{ kind: "mark", name: one.mark }} small />
          <span class="text">{one.name}</span>
          <span class="count">{one.count}</span>
        </button>
      {/each}
    </div>
  {/if}
</div>

<style>
  .filter {
    position: relative;
    flex: none;
  }

  /* The same height and surface as the dropdown an extension gets, so the
     two read as one family of controls beside the field. */
  .trigger {
    display: flex;
    gap: var(--space-1);
    align-items: center;
    height: var(--control-height);
    padding: 0 var(--space-2);
    border: 1px solid var(--hairline);
    border-radius: var(--radius-sm);
    background-color: var(--fill-1);
    background-image: var(--sheen);
    color: var(--text-2);
    font: inherit;
    font-size: var(--text-meta);
    white-space: nowrap;
    cursor: pointer;
    transition: background-color var(--motion-state) var(--ease);
  }

  .trigger:hover,
  .trigger.open {
    background-color: var(--fill-2);
    color: var(--text-1);
  }

  .trigger:focus-visible {
    outline: none;
    box-shadow: var(--focus-ring);
  }

  .label {
    max-width: 10rem;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .chevron {
    width: 10px;
    height: 10px;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.5;
    stroke-linecap: round;
    stroke-linejoin: round;
    opacity: var(--opacity-muted);
  }

  .menu {
    position: absolute;
    top: calc(100% + var(--space-1));
    right: 0;
    z-index: var(--z-menu);
    width: 220px;
    max-height: 60vh;
    padding: var(--space-1);
    overflow-y: auto;
  }

  .group {
    padding: var(--space-2) var(--space-2) var(--space-half);
    color: var(--text-3);
    font-size: var(--text-label);
    font-weight: var(--weight-medium);
    text-transform: uppercase;
    letter-spacing: 0.06em;
  }

  .option {
    display: flex;
    gap: var(--space-2);
    align-items: center;
    width: 100%;
    padding: var(--space-1) var(--space-2);
    border: 0;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--text-2);
    font: inherit;
    font-size: var(--text-meta);
    text-align: left;
    cursor: pointer;
  }

  .option:hover,
  .option:focus-visible {
    outline: none;
    background: var(--fill-2);
    color: var(--text-1);
  }

  .option[aria-checked="true"] {
    background: var(--accent-fill);
    color: var(--text-1);
  }

  .text {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .count {
    flex: none;
    color: var(--text-3);
    font-size: var(--text-label);
  }
</style>

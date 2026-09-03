<script lang="ts">
  import { groupActions, isRunnable, shortcutKeys, type ActionEntry } from "$lib/exthost/actions";
  import { popover } from "$lib/motion";

  interface Props {
    /** Already filtered. Selection counts through exactly this list. */
    actions: ActionEntry[];
    selected: number;
    /** What the list is narrowed to. Owned above, because selection is. */
    filter: string;
    onfilter: (text: string) => void;
    onselect: (index: number) => void;
    onrun: (index: number) => void;
  }

  let { actions, selected, filter, onfilter, onselect, onrun }: Props = $props();

  let field = $state<HTMLInputElement | null>(null);

  /*
   * The field takes focus the moment the panel opens.
   *
   * Without it nothing in the panel has focus, so a keystroke goes nowhere and
   * the only way through a list of eleven is the arrow keys. With it, typing
   * narrows, which is what typing does everywhere else here.
   *
   * The arrows and Enter are still handled above and prevent their default, so
   * they move the selection rather than the caret.
   */
  $effect(() => {
    field?.focus();
  });

  const groups = $derived(groupActions(actions));

  /**
   * Whether narrowing is worth offering.
   *
   * Once something has been typed it stays, or clearing the last character
   * would take the field away mid-edit along with the focus that was in it.
   */
  const showFilter = $derived(actions.length > 5 || filter.length > 0);

  /**
   * The panel renders grouped but selection is a flat index over `actions`,
   * so a group's items need to know where they sit in that flat order.
   */
  function indexOf(action: ActionEntry): number {
    return actions.findIndex((a) => a.id === action.id);
  }
</script>

<!-- Click-away closes, which is why the backdrop covers the whole window. -->
<div class="scrim" role="presentation" onclick={() => onrun(-1)}></div>

<div
  class="panel sill-menu"
  role="menu"
  tabindex="-1"
  in:popover={{ origin: "bottom right" }}
  out:popover={{ origin: "bottom right", out: true }}
>
  <!--
    Not shown until there is something to narrow. Two actions with a search box
    over them is furniture, and the panel is small enough that it shows.
  -->
  {#if showFilter}
    <div class="find">
      <input
        bind:this={field}
        value={filter}
        oninput={(e) => onfilter(e.currentTarget.value)}
        placeholder="Filter actions"
        aria-label="Filter actions"
        spellcheck="false"
        autocomplete="off"
      />
    </div>
  {/if}

  <div class="scroll">
    {#if actions.length === 0}
      <div class="empty">Nothing matches {filter}</div>
    {/if}

    {#each groups as group, g (g)}
      {#if group.section}
        <div class="section">{group.section}</div>
      {:else if g > 0}
        <div class="rule"></div>
      {/if}

      {#each group.items as action (action.id)}
        {@const index = indexOf(action)}
        <div
          class="row"
          class:selected={index === selected}
          class:destructive={action.style === "destructive"}
          role="menuitem"
          tabindex="-1"
          onmousemove={() => onselect(index)}
          onclick={(e) => {
            e.stopPropagation();
            onrun(index);
          }}
          onkeydown={(e) => e.key === "Enter" && onrun(index)}
        >
          <span class="title">{action.title}</span>
          {#if !isRunnable(action)}
            <span class="inert">no action</span>
          {/if}
          <span class="spacer"></span>
          {#if action.shortcut}
            <span class="keys">{shortcutKeys(action.shortcut).join(" ")}</span>
          {/if}
        </div>
      {/each}
    {/each}

    {#if actions.length === 0}
      <div class="empty">No actions</div>
    {/if}
  </div>
</div>

<style>
  .scrim {
    position: fixed;
    inset: 0;
    z-index: var(--z-panel-scrim);
  }

  /* Anchored bottom right, above the footer, the way a launcher's action
     menu rises out of its own affordance. `bottom` clears the 40px footer
     and lands the panel on the same right edge as the action pill. */
  .panel {
    position: fixed;
    right: var(--space-2);
    bottom: 44px;
    z-index: var(--z-panel);
    width: 320px;
    max-height: 60vh;
    overflow: hidden;
    display: flex;
    flex-direction: column;
  }

  .scroll {
    overflow-y: auto;
    padding: var(--space-1);
    scrollbar-width: thin;
    scrollbar-color: var(--scrollbar-thumb) transparent;
  }

  .section {
    padding: var(--space-2) var(--space-2) var(--space-1);
    font-size: var(--text-group);
    font-weight: var(--weight-medium);
    color: var(--text-3);
  }

  .rule {
    height: 1px;
    margin: var(--space-1);
    background: var(--hairline);
  }

  .row {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    height: 32px;
    padding: 0 var(--space-2);
    border-radius: var(--radius-md);
    cursor: default;
    transition: background-color var(--motion-state) var(--ease);
  }

  .row.selected {
    background-color: var(--accent-fill);
  }

  .row.destructive .title {
    color: var(--danger);
  }

  .title {
    font-size: var(--text-body);
    font-weight: var(--weight-body);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .inert {
    font-size: var(--text-meta);
    color: var(--text-3);
  }

  .spacer {
    flex: 1;
  }

  .find {
    padding: var(--space-2);
    border-bottom: 1px solid var(--hairline);
  }

  .find input {
    width: 100%;
    padding: var(--space-1) var(--space-2);
    border: 0;
    border-radius: var(--radius-sm);
    background: var(--fill-1);
    color: var(--text-1);
    font: inherit;
    font-size: var(--text-meta);
    outline: none;
  }

  .find input::placeholder {
    color: var(--text-3);
  }

  .keys {
    flex: none;
    font-size: var(--text-meta);
    font-weight: var(--weight-medium);
    color: var(--text-3);
  }

  .empty {
    padding: var(--space-5) var(--space-2);
    text-align: center;
    color: var(--text-3);
    font-size: var(--text-body);
  }
</style>

<script lang="ts">
  import { groupActions, isRunnable, shortcutKeys, type ActionEntry } from "$lib/exthost/actions";

  interface Props {
    actions: ActionEntry[];
    selected: number;
    onselect: (index: number) => void;
    onrun: (index: number) => void;
  }

  let { actions, selected, onselect, onrun }: Props = $props();

  const groups = $derived(groupActions(actions));

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

<div class="panel sill-menu" role="menu" tabindex="-1">
  <div class="scroll">
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
    z-index: 10;
  }

  /* Anchored bottom right, above the footer, the way a launcher's action
     menu rises out of its own affordance. */
  .panel {
    position: fixed;
    right: 8px;
    bottom: 36px;
    z-index: 11;
    width: 320px;
    max-height: 60vh;
    overflow: hidden;
    display: flex;
    flex-direction: column;
  }

  .scroll {
    overflow-y: auto;
    padding: 5px;
    scrollbar-width: thin;
    scrollbar-color: rgba(var(--accent-rgb), 0.3) transparent;
  }

  .section {
    padding: 8px 9px 4px;
    font-size: var(--text-group);
    font-weight: 500;
    color: var(--text-faint);
  }

  .rule {
    height: 1px;
    margin: 5px 4px;
    background: var(--hairline);
  }

  .row {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 32px;
    padding: 0 9px;
    border-radius: var(--radius-sm);
    cursor: default;
    transition: background-color 0.15s var(--ease);
  }

  .row.selected {
    background-color: var(--surface);
  }

  .row.destructive .title {
    color: var(--accent-red);
  }

  .title {
    font-size: var(--text-row);
    font-weight: var(--weight-row);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .inert {
    font-size: var(--text-meta);
    color: var(--text-faint);
  }

  .spacer {
    flex: 1;
  }

  .keys {
    flex: none;
    font-size: var(--text-meta);
    font-weight: 500;
    color: var(--text-faint);
  }

  .empty {
    padding: 18px 9px;
    text-align: center;
    color: var(--text-faint);
    font-size: var(--text-row);
  }
</style>

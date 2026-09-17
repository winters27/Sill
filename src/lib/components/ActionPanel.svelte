<script lang="ts">
  import {
    actionIcon,
    groupActions,
    isRunnable,
    shortcutKeys,
    type ActionEntry,
  } from "$lib/exthost/actions";
  import ExtIcon from "./ExtIcon.svelte";
  import { noMatch, standing } from "$lib/instead";
  import Instead from "./Instead.svelte";
  import { popover } from "$lib/motion";
  import { itemId } from "$lib/results";

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
  let panel = $state<HTMLDivElement | null>(null);

  /** The menu, for `aria-controls` and for the item ids below. */
  const MENU = "sill-actions";

  /*
   * The field takes focus the moment the panel opens.
   *
   * Without it nothing in the panel has focus, so a keystroke goes nowhere and
   * the only way through a list of eleven is the arrow keys. With it, typing
   * narrows, which is what typing does everywhere else here.
   *
   * The arrows and Enter are still handled above and prevent their default, so
   * they move the selection rather than the caret.
   *
   * The panel itself takes focus when there is no field, which is the common
   * case: five actions or fewer draws no filter. That is not about keystrokes,
   * which the window catches either way. It is that
   * `aria-activedescendant` is only read from the element that HAS focus, so
   * with focus left back on the search field nothing announced which action
   * the highlight was on. Focus goes back to the field when the panel closes,
   * which the launcher already does for every one of the nine ways it closes.
   */
  $effect(() => {
    if (field) field.focus();
    else panel?.focus();
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

  /*
   * One state, drawn once.
   *
   * This panel used to test `actions.length === 0` twice, once above the list
   * and once below it, so an empty panel drew "Nothing matches" and "No
   * actions" at the same time and the two sentences contradicted each other.
   * Both tests were true, both were reasonable on their own, and nothing could
   * fail: an empty panel is a rare screen and neither branch was wrong.
   *
   * Deriving the standing is what makes that shape impossible rather than
   * merely fixed, because one value cannot be two of them.
   */
  const showing = $derived(
    standing({ failed: false, loading: false, count: actions.length }),
  );
</script>

<!-- Click-away closes, which is why the backdrop covers the whole window. -->
<div class="scrim" role="presentation" onclick={() => onrun(-1)}></div>

<!--
  The panel names the action under the highlight.

  `aria-activedescendant` sits on both the panel and the filter field because
  either of them can hold focus, and it is only read from whichever one does.
  Pointing at an id that is not rendered is harmless; leaving it off the one
  that has focus is silence.
-->
<div
  id={MENU}
  bind:this={panel}
  class="panel sill-menu"
  role="menu"
  tabindex="-1"
  aria-label="Actions"
  aria-activedescendant={actions.length ? itemId(MENU, selected) : undefined}
  in:popover={{ origin: "bottom right" }}
  out:popover={{ origin: "bottom right", out: true }}
>
  <div class="scroll" role="presentation">
    {#each groups as group, g (g)}
      {#if group.section}
        <div class="section" role="presentation">{group.section}</div>
      {:else if g > 0}
        <div class="rule" role="separator"></div>
      {/if}

      {#each group.items as action (action.id)}
        {@const index = indexOf(action)}
        <!--
          The mark, resolved here rather than in the row below it because a
          `{@const}` may only be the immediate child of a block.
        -->
        {@const mark = actionIcon(action)}
        <div
          id={itemId(MENU, index)}
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
          <!--
            Drawn even when there is nothing to draw. A row missing its
            picture in a list where the rest have one reads as a broken row;
            an empty 16px box reads as an action nobody has drawn yet, which
            is what it is.
          -->
          <span class="mark" aria-hidden="true">
            {#if mark}<ExtIcon icon={mark} small />{/if}
          </span>
          <span class="title">{action.title}</span>
          {#if !isRunnable(action)}
            <span class="inert">no action</span>
          {/if}
          <span class="spacer"></span>
          {#if action.shortcut}
            <!--
              One cap per key, the same `.sill-key` the chin draws. The panel
              used to join them into one grey string, so the two surfaces that
              show a chord showed it two different ways and the one that rises
              out of the other was the plainer of them.
            -->
            <span class="keys">
              {#each shortcutKeys(action.shortcut) as key, k (k)}
                <span class="sill-key">{key}</span>
              {/each}
            </span>
          {/if}
        </div>
      {/each}
    {/each}

    <!-- Inline rather than the pane recipe: this popover is about as tall as
         the mark and the space around it would be on their own. -->
    <Instead
      tone={showing}
      inline
      headline={filter ? noMatch(filter, "actions") : "No actions"}
      hint={filter ? "" : "Nothing here can be done to this row."}
    />
  </div>

  <!--
    Under the list, not over it.

    The panel rises out of the chin and the field is the part nearest it, so
    the caret lands where the eye already is rather than at the far end of a
    list somebody has to look past. It is also where every launcher that has
    this control puts it.

    Not shown until there is something to narrow. Two actions with a search
    box under them is furniture, and the panel is small enough that it shows.

    Written out here rather than declared as a `{#snippet}` above the list and
    rendered here. It was that for one commit and the field stopped taking
    focus on open: the `$effect` that focuses it reads `field`, and a binding
    inside a snippet is not set by the time that effect first runs. Typing went
    nowhere and the only way through eleven actions was the arrow keys, which
    is the exact failure the effect exists to prevent.
  -->
  {#if showFilter}
    <div class="find" role="presentation">
      <input
        bind:this={field}
        value={filter}
        oninput={(e) => onfilter(e.currentTarget.value)}
        placeholder="Search for actions..."
        aria-label="Filter actions"
        role="combobox"
        aria-expanded="true"
        aria-haspopup="menu"
        aria-controls={MENU}
        aria-activedescendant={actions.length ? itemId(MENU, selected) : undefined}
        aria-autocomplete="list"
        spellcheck="false"
        autocomplete="off"
      />
    </div>
  {/if}
</div>

<style>
  .scrim {
    position: fixed;
    inset: 0;
    z-index: var(--z-panel-scrim);
  }

  /* Anchored bottom right, above the footer, the way a launcher's action
     menu rises out of its own affordance. `bottom` clears the chin by one
     step, from the same token Rust sizes the window with, and lands the
     panel on the same right edge as the action pill. */
  .panel {
    position: fixed;
    right: var(--space-2);
    bottom: calc(var(--chin-height) + var(--space-1));
    z-index: var(--z-panel);
    width: 400px;
    max-height: 60vh;
    overflow: hidden;
    display: flex;
    flex-direction: column;
  }

  .scroll {
    overflow-y: auto;
    padding: var(--space-1);
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

  /* The mark column.

     Fixed width and always present, so the titles line up whether or not a
     given action has a picture. A column that collapses on the rows with
     nothing to draw is a ragged left edge, which is the thing an icon
     column is supposed to fix. */
  .mark {
    display: grid;
    place-items: center;
    flex: none;
    width: var(--icon-tile-xs);
    height: var(--icon-tile-xs);
    color: var(--text-2);
  }

  /* Both halves, not just the words. `ExtIcon` draws a mark in
     `currentColor`, so the colour on the row reaches the picture too and a
     destructive row reads as one before it is read. */
  .row.destructive .title,
  .row.destructive .mark {
    color: var(--danger);
  }

  .title {
    font-size: var(--text-body);
    font-weight: var(--weight-body);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  /* Sits between a title that truncates and keys that do not, so it must
     not be the thing that wraps a 32px row. */
  .inert {
    flex: none;
    white-space: nowrap;
    font-size: var(--text-meta);
    color: var(--text-3);
  }

  .spacer {
    flex: 1;
  }

  /* Above the field rather than below it, because the field is the last
     thing in the panel now. `margin-top: auto` is what keeps it against the
     bottom edge when the list is shorter than the panel. */
  .find {
    flex: none;
    margin-top: auto;
    padding: var(--space-1) var(--space-2);
    border-top: 1px solid var(--hairline);
  }

  /* No fill. A filled box inside a popover is a second surface sitting on a
     surface, and the rule above it already says where the list stops. */
  .find input {
    width: 100%;
    height: 28px;
    padding: 0 var(--space-1);
    border: 0;
    background: transparent;
    color: var(--text-1);
    font: inherit;
    font-size: var(--text-meta);
    outline: none;
  }

  .find input::placeholder {
    color: var(--text-3);
  }

  .keys {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    flex: none;
  }
</style>

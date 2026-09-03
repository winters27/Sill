<script lang="ts">
  import type { ElementNode, ViewTree } from "$lib/exthost/tree";
  import Instead from "./Instead.svelte";
  import { standing } from "$lib/instead";
  import { LISTBOX, optionId } from "$lib/results";

  interface Props {
    tree: ViewTree;
    node: ElementNode;
    selected: number;
    onselect: (index: number) => void;
    onrun: (index: number) => void;
  }

  let { tree, node, selected, onselect, onrun }: Props = $props();

  /**
   * Sections are flattened into the same index space as loose items, because
   * selection moves through what the user sees as one list regardless of how
   * the extension grouped it.
   */
  /**
   * A section heading or a selectable item, told apart by `kind`.
   *
   * Two shapes rather than one with an optional index, so the index is a
   * number wherever the markup has narrowed to an item. It was optional, and
   * every use of it carried an `index !== undefined` guard that could never be
   * false; the row id below is the first thing that would have been silently
   * wrong if one of those guards had ever gone the other way.
   */
  type Row =
    | { kind: "section"; node: ElementNode }
    | { kind: "item"; node: ElementNode; index: number };

  const rows = $derived.by(() => {
    const out: Row[] = [];
    let index = 0;

    const pushItem = (item: ElementNode) => {
      out.push({ kind: "item", node: item, index: index++ });
    };

    for (const child of tree.elementChildren(node)) {
      if (child.tag === "List.Section") {
        out.push({ kind: "section", node: child });
        for (const item of tree.elementChildren(child)) {
          if (item.tag === "List.Item") pushItem(item);
        }
      } else if (child.tag === "List.Item") {
        pushItem(child);
      }
    }

    return out;
  });

  const str = (n: ElementNode, key: string): string => {
    const value = n.props[key];
    return typeof value === "string" ? value : "";
  };
</script>

<!--
  Named by the field above it, the same way the root list is.

  The id is the one the search field points `aria-controls` at, and the row
  ids below are what its `aria-activedescendant` names. Without them somebody
  arrowing through an extension's list heard the field announced once and then
  silence, because focus never leaves the field and nothing said which row had
  moved under the highlight.
-->
<div
  id={LISTBOX}
  class="sill-list"
  role="listbox"
  tabindex="-1"
  aria-label={str(node, "navigationTitle") || "Results"}
>
  {#each rows as row (row.node.id)}
    {#if row.kind === "section"}
      <!-- A label between options, not an option. Left unmarked it is a child
           of the listbox with no role, which some readers count as a row. -->
      <div class="section" role="presentation">
        {str(row.node, "title")}
        {#if str(row.node, "subtitle")}
          <span class="section-sub">{str(row.node, "subtitle")}</span>
        {/if}
      </div>
    {:else}
      <div
        id={optionId(row.index)}
        class="sill-row"
        class:selected={row.index === selected}
        role="option"
        aria-selected={row.index === selected}
        tabindex="-1"
        onmousemove={() => onselect(row.index)}
        onclick={() => onrun(row.index)}
        onkeydown={(e) => e.key === "Enter" && onrun(row.index)}
      >
        <!--
          Title and subtitle stay on ONE line, side by side.

          This is not the root list's stacked anatomy and must not become it.
          Raycast documents `subtitle` as "an optional subtitle displayed next
          to the main title", and `accessories` as the separate prop that
          renders on the right, so an extension author writing a subtitle
          means "beside", not "under". The row chrome is shared with the root
          list; the content layout follows the API.
        -->
        <span class="title">{str(row.node, "title")}</span>
        {#if str(row.node, "subtitle")}
          <span class="subtitle">{str(row.node, "subtitle")}</span>
        {/if}
      </div>
    {/if}
  {/each}

  <Instead
    tone={standing({ failed: false, loading: false, count: rows.length })}
    headline="No results"
    hint="This command found nothing to show."
  />
</div>

<style>
  /* Matches `.sill-group` in the root list: a label, not a rule. */
  .section {
    display: flex;
    align-items: flex-end;
    height: var(--control-height);
    padding: 0 var(--space-3) var(--space-2);
    font-size: var(--text-group);
    font-weight: var(--weight-medium);
    color: var(--text-3);
  }

  /* An extension's own words about the section, so it is read. --text-4 is
     declared decorative-only and is not the step for something that says what
     a group of rows is. */
  .section-sub {
    margin-left: var(--space-1);
    font-weight: 400;
    color: var(--text-3);
  }

  .title {
    color: var(--text-1);
    font-size: var(--text-body);
    font-weight: var(--weight-body);
    line-height: var(--line-body);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    flex: none;
    max-width: 60%;
  }

  /* Beside the title, quieter. Same size, because a second type size on a row
     is what made the root list read as unrelated fragments sharing a line. */
  .subtitle {
    color: var(--text-3);
    font-size: var(--text-body);
    line-height: var(--line-body);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
</style>

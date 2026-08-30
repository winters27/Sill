<script lang="ts">
  import type { ElementNode, ViewTree } from "$lib/exthost/tree";

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
  interface Row {
    kind: "section" | "item";
    node: ElementNode;
    /** Only items are selectable, so only items carry an index. */
    index?: number;
  }

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

<div class="sill-list" role="listbox" tabindex="-1" aria-label={str(node, "navigationTitle") || "Results"}>
  {#each rows as row (row.node.id)}
    {#if row.kind === "section"}
      <div class="section">
        {str(row.node, "title")}
        {#if str(row.node, "subtitle")}
          <span class="section-sub">{str(row.node, "subtitle")}</span>
        {/if}
      </div>
    {:else}
      <div
        class="sill-row"
        class:selected={row.index === selected}
        role="option"
        aria-selected={row.index === selected}
        tabindex="-1"
        onmousemove={() => row.index !== undefined && onselect(row.index)}
        onclick={() => row.index !== undefined && onrun(row.index)}
        onkeydown={(e) => e.key === "Enter" && row.index !== undefined && onrun(row.index)}
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

  {#if rows.length === 0}
    <div class="sill-empty">
      <img src="/sill.png" alt="" width="32" height="32" draggable="false" />
      <span class="headline">No results</span>
      <span class="hint">This command found nothing to show.</span>
    </div>
  {/if}
</div>

<style>
  /* Matches `.sill-group` in the root list: a label, not a rule. */
  .section {
    display: flex;
    align-items: flex-end;
    height: 30px;
    padding: 0 var(--space-3) var(--space-2);
    font-size: var(--text-group);
    font-weight: var(--weight-medium);
    color: var(--text-3);
  }

  .section-sub {
    margin-left: var(--space-1);
    font-weight: 400;
    color: var(--text-4);
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

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
        <span class="title">{str(row.node, "title")}</span>
        {#if str(row.node, "subtitle")}
          <span class="subtitle">{str(row.node, "subtitle")}</span>
        {/if}
      </div>
    {/if}
  {/each}

  {#if rows.length === 0}
    <div class="sill-empty">No results</div>
  {/if}
</div>

<style>
  /* Matches `.sill-group` in the root list: a label, not a rule. */
  .section {
    padding: 10px 12px 4px;
    font-size: var(--text-group);
    font-weight: 500;
    color: var(--text-faint);
  }

  .section-sub {
    margin-left: 6px;
    font-weight: 400;
  }

  .title {
    color: var(--core-foreground);
    font-size: var(--text-row);
    font-weight: var(--weight-row);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .subtitle {
    color: var(--text-faint);
    font-size: var(--text-row);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
</style>

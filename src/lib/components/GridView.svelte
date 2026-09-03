<script lang="ts">
  import type { ElementNode, ViewTree } from "$lib/exthost/tree";
  import Instead from "./Instead.svelte";
  import { standing } from "$lib/instead";
  import { LISTBOX, optionId } from "$lib/results";

  interface Props {
    tree: ViewTree;
    node: ElementNode;
    version: number;
    selected: number;
    onselect: (index: number) => void;
    onrun: (index: number) => void;
  }

  let { tree, node, version, selected, onselect, onrun }: Props = $props();

  /** Raycast's column counts; the prop is a number of columns, not a width. */
  const columns = $derived.by(() => {
    version;
    const value = node.props.columns;
    return typeof value === "number" && value > 0 ? Math.min(value, 8) : 5;
  });

  /**
   * A section heading or a selectable cell, told apart by `kind`.
   *
   * Two shapes rather than one with an optional index, so a cell's index is a
   * number wherever the markup has narrowed to one. The row id below has to be
   * a real index or `aria-activedescendant` points at nothing and the
   * announcement stops, which is a failure with no visible symptom.
   */
  type Cell =
    | { kind: "section"; node: ElementNode }
    | { kind: "item"; node: ElementNode; index: number };

  /**
   * Sections and items share one index space, as in the list, so selection
   * moves through what the user sees rather than through the markup.
   */
  const cells = $derived.by(() => {
    version;
    const out: Cell[] = [];
    let index = 0;

    for (const child of tree.elementChildren(node)) {
      if (child.tag === "Grid.Section") {
        out.push({ kind: "section", node: child });
        for (const item of tree.elementChildren(child)) {
          if (item.tag === "Grid.Item") out.push({ kind: "item", node: item, index: index++ });
        }
      } else if (child.tag === "Grid.Item") {
        out.push({ kind: "item", node: child, index: index++ });
      }
    }

    return out;
  });

  const str = (n: ElementNode, key: string): string => {
    const v = n.props[key];
    return typeof v === "string" ? v : "";
  };

  /**
   * What to draw in a cell.
   *
   * `content` is usually an image source, but extensions also pass a bare
   * string for emoji and similar, which is worth rendering directly rather
   * than treating as a broken image.
   */
  function contentOf(item: ElementNode): { text?: string; src?: string } {
    const content = item.props.content;

    if (typeof content === "string") {
      return content.startsWith("http") || content.includes("/") || content.includes(".")
        ? { src: content }
        : { text: content };
    }

    if (content && typeof content === "object") {
      const source = (content as Record<string, unknown>).source;
      if (typeof source === "string") return { src: source };
    }

    return { text: str(item, "title").slice(0, 2) };
  }
</script>

<!--
  A grid of pictures is still one list to walk, so it stays a listbox.

  No `aria-orientation`. It is laid out in columns, but the launcher moves the
  selection with Up and Down only, and saying "horizontal" would tell somebody
  to press keys that do nothing here. What is drawn and what the keys do are
  different questions, and the reader is answering the second.

  The id is the one the search field points at, and every cell carries the id
  that field names when the cell is highlighted.
-->
<div
  id={LISTBOX}
  class="grid-scroll"
  role="listbox"
  tabindex="-1"
  aria-label={str(node, "navigationTitle") || "Results"}
>
  <div class="grid" style="--columns: {columns}">
    {#each cells as cell (cell.node.id)}
      {#if cell.kind === "section"}
        <div class="section" role="presentation">{str(cell.node, "title")}</div>
      {:else}
        {@const content = contentOf(cell.node)}
        <div
          id={optionId(cell.index)}
          class="cell"
          class:selected={cell.index === selected}
          role="option"
          aria-selected={cell.index === selected}
          tabindex="-1"
          onmousemove={() => onselect(cell.index)}
          onclick={() => onrun(cell.index)}
          onkeydown={(e) => e.key === "Enter" && onrun(cell.index)}
        >
          <div class="tile">
            {#if content.src}
              <img src={content.src} alt={str(cell.node, "title")} />
            {:else}
              <span class="glyph">{content.text}</span>
            {/if}
          </div>
          {#if str(cell.node, "title")}
            <div class="title">{str(cell.node, "title")}</div>
          {/if}
          {#if str(cell.node, "subtitle")}
            <div class="subtitle">{str(cell.node, "subtitle")}</div>
          {/if}
        </div>
      {/if}
    {/each}
  </div>

  <Instead
    tone={standing({ failed: false, loading: false, count: cells.length })}
    headline="Nothing to show"
    hint="This command returned an empty grid."
  />
</div>

<style>
  .grid-scroll {
    flex: 1;
    overflow-y: auto;
    padding: var(--space-2);
    outline: none;
    scrollbar-width: thin;
    scrollbar-color: var(--scrollbar-thumb) transparent;
  }

  .grid {
    display: grid;
    grid-template-columns: repeat(var(--columns), 1fr);
    gap: var(--space-2);
  }

  /* A section heading interrupts the grid rather than sitting in one cell.
     Sentence case, matching `.sill-group`: this is a list separator, not a
     settings section label. */
  .section {
    grid-column: 1 / -1;
    padding: var(--space-2) var(--space-1) var(--space-1);
    font-size: var(--text-group);
    font-weight: var(--weight-medium);
    color: var(--text-3);
  }

  /*
   * The cell is only the selection wash. The tile inside it is the object that
   * carries the bevel, and stacking a second bevel plus a border out here read
   * as a heavy double frame around every item.
   */
  .cell {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    padding: var(--space-2);
    border-radius: var(--radius-md);
    cursor: default;
    /* Scoped, and backdrop-filter is never animated: see the WebView2 note. */
    transition:
      background-color var(--motion-travel) var(--ease),
      box-shadow var(--motion-state) var(--ease);
  }

  .cell:hover:not(.selected) {
    background-color: var(--fill-1);
  }

  /*
   * The same pair a selected row takes: the accent wash and one light catch.
   *
   * `--catch` is a single 1px inset on the top edge, which is not the double
   * frame the note above refuses. That note was about `--bevel-tile`, which
   * has an edge on every side and is already on the tile inside. Without the
   * catch a selected cell was the one selected thing in Sill lit differently
   * from every other, and a grid beside a list showed the difference.
   */
  .cell.selected {
    background-color: var(--accent-fill);
    box-shadow: var(--catch);
  }

  .tile {
    display: grid;
    place-items: center;
    aspect-ratio: 1;
    border-radius: var(--radius-sm);
    background-color: color-mix(in srgb, var(--grid-item-background) 55%, transparent);
    background-image: var(--sheen);
    box-shadow: var(--bevel-tile);
    overflow: hidden;
  }

  .tile img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .glyph {
    font-size: var(--glyph-lg);
    line-height: 1;
  }

  .title {
    font-size: var(--text-meta);
    color: var(--text-1);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .subtitle {
    font-size: var(--text-meta);
    color: var(--text-3);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
</style>

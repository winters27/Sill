<script lang="ts">
  import type { ElementNode, ViewTree } from "$lib/exthost/tree";

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

  interface Cell {
    kind: "section" | "item";
    node: ElementNode;
    index?: number;
  }

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

<div class="grid-scroll" role="listbox" tabindex="-1" aria-label="Grid">
  <div class="grid" style="--columns: {columns}">
    {#each cells as cell (cell.node.id)}
      {#if cell.kind === "section"}
        <div class="section">{str(cell.node, "title")}</div>
      {:else}
        {@const content = contentOf(cell.node)}
        <div
          class="cell"
          class:selected={cell.index === selected}
          role="option"
          aria-selected={cell.index === selected}
          tabindex="-1"
          onmousemove={() => cell.index !== undefined && onselect(cell.index)}
          onclick={() => cell.index !== undefined && onrun(cell.index)}
          onkeydown={(e) => e.key === "Enter" && cell.index !== undefined && onrun(cell.index)}
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

  {#if cells.length === 0}
    <div class="sill-empty">Nothing to show</div>
  {/if}
</div>

<style>
  .grid-scroll {
    flex: 1;
    overflow-y: auto;
    padding: 10px;
    outline: none;
    scrollbar-width: thin;
    scrollbar-color: var(--hairline) transparent;
  }

  .grid {
    display: grid;
    grid-template-columns: repeat(var(--columns), 1fr);
    gap: 8px;
  }

  /* A section heading interrupts the grid rather than sitting in one cell. */
  .section {
    grid-column: 1 / -1;
    padding: 8px 4px 2px;
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--text-faint);
  }

  /*
   * The cell is only the selection wash. The tile inside it is the object that
   * carries the bevel, and stacking a second bevel plus a border out here read
   * as a heavy double frame around every item.
   */
  .cell {
    display: flex;
    flex-direction: column;
    gap: 5px;
    padding: 6px;
    border-radius: var(--radius);
    cursor: default;
    /* Scoped, and backdrop-filter is never animated: see the WebView2 note. */
    transition: background-color 0.22s var(--ease);
  }

  .cell:hover:not(.selected) {
    background-color: rgba(var(--accent-rgb), 0.07);
  }

  .cell.selected {
    background-color: var(--surface);
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
    font-size: 26px;
    line-height: 1;
  }

  .title {
    font-size: 12px;
    color: var(--core-foreground);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .subtitle {
    font-size: 11px;
    color: var(--text-faint);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
</style>

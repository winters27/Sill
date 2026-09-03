<script lang="ts">
  /**
   * A page of prose, with the facts beside it.
   *
   * One component for both of Raycast's detail surfaces. `<Detail>` is a whole
   * view and `<List.Item.Detail>` is the right-hand half of a list, and they
   * are the same thing drawn at two widths: markdown, then a metadata panel.
   * Two components would be two answers to "what does a Link row look like",
   * and they would drift the first time one of them was touched.
   *
   * ## Why the markdown goes through `Markdown`
   *
   * It used to be a `<div>` holding the source. An extension that wrote a
   * heading got a hash on screen, one that wrote a table got pipes, and one
   * that wrote a link got the brackets. `Markdown` already parses all of it
   * into elements, and it never hands the document a string of HTML, so an
   * extension cannot write markup into Sill's window by writing markup into
   * its own detail text.
   */
  import type { ElementNode, ViewTree } from "$lib/exthost/tree";
  import { metadataOf, type MetaRow } from "$lib/exthost/present";
  import ExtIcon from "./ExtIcon.svelte";
  import Markdown from "./Markdown.svelte";
  import { hint } from "$lib/hint";

  interface Props {
    tree: ViewTree;
    /** A `Detail` or a `List.Item.Detail`. */
    node: ElementNode;
    version: number;
    /** Narrow, for the half of a list rather than a whole view. */
    beside?: boolean;
  }

  let { tree, node, version, beside = false }: Props = $props();

  /**
   * The document, from the prop or from the children.
   *
   * Raycast documents `markdown` as a prop, and extensions also write the text
   * between the tags. Both arrive, and reading only one of them is a blank
   * page for whichever half of the ecosystem was not chosen.
   */
  const text = $derived.by(() => {
    version;
    const prop = node.props.markdown;
    return typeof prop === "string" ? prop : tree.text(node);
  });

  const rows = $derived.by((): MetaRow[] => {
    version;
    const panel = tree.slot(node, "metadata");
    return panel ? metadataOf(tree, panel) : [];
  });
</script>

<div class="detail" class:beside>
  {#if text}
    <div class="prose md"><Markdown {text} /></div>
  {/if}

  {#if rows.length}
    <!-- A description list, because that is what it is: a column of terms and
         the value each one has. Read as pairs rather than as a run of
         sentences, which is what a stack of divs would be. -->
    <dl class="meta">
      {#each rows as row, at (at)}
        {#if row.kind === "separator"}
          <div class="rule" role="presentation"></div>
        {:else}
          <dt>{row.title}</dt>
          <dd>
            {#if row.kind === "label"}
              {#if row.icon}
                <ExtIcon icon={row.icon} small />
              {/if}
              <span class="value">{row.text}</span>
            {:else if row.kind === "link"}
              <!-- Not an anchor. Nothing in this window navigates, and a link
                   that looks live and does nothing is worse than one that
                   says what it points at. The URL is the tooltip, which is
                   the same treatment every other truncated path gets. -->
              <span class="value link" use:hint={row.url}>{row.text}</span>
            {:else}
              <span class="tags">
                {#each row.tags as tag, n (n)}
                  <span class="tag" style={tag.tint ? `color: ${tag.tint}` : undefined}>
                    {#if tag.icon}
                      <ExtIcon icon={tag.icon} small />
                    {/if}
                    {tag.text}
                  </span>
                {/each}
              </span>
            {/if}
          </dd>
        {/if}
      {/each}
    </dl>
  {/if}
</div>

<style>
  .detail {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
    overflow-y: auto;
    padding: var(--space-4);
    user-select: text;
    scrollbar-width: thin;
    scrollbar-color: var(--scrollbar-thumb) transparent;
  }

  /*
   * The half beside a list, which is a column rather than a page.
   *
   * A hairline down its left edge and not a filled panel: the rows and the
   * detail are one surface with a division in it, and shading one half turns
   * a list into two windows sharing a frame.
   */
  .detail.beside {
    border-left: 1px solid var(--hairline);
    padding: var(--space-3);
    gap: var(--space-3);
  }

  .prose {
    color: var(--text-1);
    font-size: var(--text-body);
  }

  .meta {
    display: grid;
    grid-template-columns: auto 1fr;
    align-items: baseline;
    gap: var(--space-1) var(--space-3);
    margin: 0;
  }

  /* Stacked once the pane is a column, because a term and its value side by
     side in a narrow pane leaves neither enough room to be read. */
  .detail.beside .meta {
    grid-template-columns: 1fr;
    gap: var(--space-half);
  }

  dt {
    color: var(--text-3);
    font-size: var(--text-meta);
    white-space: nowrap;
  }

  dd {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    margin: 0;
    min-width: 0;
    color: var(--text-1);
    font-size: var(--text-body);
  }

  .detail.beside dd {
    margin-bottom: var(--space-2);
  }

  .value {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .link {
    color: var(--info);
  }

  .rule {
    grid-column: 1 / -1;
    height: 1px;
    background: var(--hairline);
    margin: var(--space-2) 0;
  }

  .tags {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1);
  }

  /*
   * A pill, in the colour the extension asked for and nothing else.
   *
   * The wash is `color-mix` over the pill's own colour, so an untinted tag is
   * the ordinary fill and a tinted one is a tint of the same shape rather than
   * a second design. One rule, whatever the extension passed.
   */
  .tag {
    display: inline-flex;
    align-items: center;
    gap: var(--space-half);
    padding: 0 var(--space-2);
    border-radius: var(--radius-pill);
    background-color: color-mix(in srgb, currentColor 16%, transparent);
    font-size: var(--text-meta);
    line-height: var(--line-meta);
    color: var(--text-2);
  }
</style>

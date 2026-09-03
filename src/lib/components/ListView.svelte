<script lang="ts">
  import type { ElementNode, ViewTree } from "$lib/exthost/tree";
  import type { Row } from "$lib/exthost/search";
  import {
    accessoriesOf,
    detailOf,
    emptyViewOf,
    iconOf,
    showsDetail,
  } from "$lib/exthost/present";
  import DetailPane from "./DetailPane.svelte";
  import ExtIcon from "./ExtIcon.svelte";
  import Instead from "./Instead.svelte";
  import { whileEmpty } from "$lib/instead";
  import { hint } from "$lib/hint";
  import { LISTBOX, optionId } from "$lib/results";

  interface Props {
    /** The whole tree, for the parts of a row that arrive as subtrees. */
    tree: ViewTree;
    node: ElementNode;
    /** Bumped on every op batch, so what is read out of the tree re-reads. */
    version: number;
    /**
     * The rows to draw, already flattened and already narrowed.
     *
     * Built by the page rather than here, because the page has to walk the
     * same sequence to know what Enter runs and how far the arrow keys go.
     * This component used to derive its own from the tree, by the same rules,
     * written twice; the moment a filter narrowed one of the two they stopped
     * agreeing and the highlight pointed at a different row from the one that
     * would have run.
     *
     * Sections share the item index space, because selection moves through
     * what the reader sees as one list however the extension grouped it.
     */
    rows: Row[];
    /** What is in the field, so an empty list can name what emptied it. */
    query: string;
    /** Whether the extension says an answer is still coming. */
    loading: boolean;
    selected: number;
    onselect: (index: number) => void;
    onrun: (index: number) => void;
  }

  let { tree, node, version, rows, query, loading, selected, onselect, onrun }: Props = $props();

  const str = (n: ElementNode, key: string): string => {
    const value = n.props[key];
    if (typeof value === "string") return value;
    // Raycast lets a title be `{ value, tooltip }`, and a row written that way
    // used to draw as nothing at all.
    if (value && typeof value === "object") {
      const inner = (value as { value?: unknown }).value;
      if (typeof inner === "string") return inner;
    }
    return "";
  };

  /**
   * The extension's own words for an empty list, when it wrote any.
   *
   * `List.EmptyView` is Raycast's way of saying "here is what my list means
   * when it has nothing in it", and nobody knows that better than the command
   * that produced it. It fills in the one empty-state recipe rather than
   * getting a design of its own: `Instead` still draws it, still centres it,
   * still refuses a spinner.
   */
  const declared = $derived.by(() => {
    version;
    return emptyViewOf(tree, node);
  });

  /**
   * What the pane says when it has no rows, which is three different things.
   *
   * The choice lives in `$lib/instead` with the rest of it, so a list and a
   * grid say the same thing about the same situation and the rule can be
   * tested without mounting anything.
   *
   * The extension's own words replace the last of the three only. A command
   * still fetching says it is still fetching, and a search that matched
   * nothing names the word that matched nothing, because neither of those is
   * what an `EmptyView` is about: it describes an empty result, not a wait and
   * not a filter.
   */
  const saying = $derived.by(() => {
    const words = whileEmpty({ failed: false, loading, count: rows.length }, query, {
      headline: declared?.headline || "No results",
      hint: declared?.hint || "This command found nothing to show.",
    });
    return words;
  });

  /** The pane beside the rows, which belongs to whichever row is highlighted. */
  const detail = $derived.by(() => {
    version;
    if (!showsDetail(node)) return undefined;
    const row = rows.find((one) => one.kind === "item" && one.index === selected);
    return row ? detailOf(tree, row.node) : undefined;
  });
</script>

<!--
  Named by the field above it, the same way the root list is.

  The id is the one the search field points `aria-controls` at, and the row
  ids below are what its `aria-activedescendant` names. Without them somebody
  arrowing through an extension's list heard the field announced once and then
  silence, because focus never leaves the field and nothing said which row had
  moved under the highlight.
-->
<!--
  The rows and the pane beside them, when there is one.

  A wrapper only when `isShowingDetail` asked for one. Without the flag this
  draws exactly what it drew before: the listbox and nothing around it.
-->
<div class="split" class:showing={detail !== undefined}>
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
        {#if iconOf(row.node.props.icon)}
          {@const icon = iconOf(row.node.props.icon)}
          {#if icon}
            <ExtIcon {icon} />
          {/if}
        {/if}

        <span class="title">{str(row.node, "title")}</span>
        {#if !detail && str(row.node, "subtitle")}
          <span class="subtitle">{str(row.node, "subtitle")}</span>
        {/if}

        <!--
          Everything else the row says, pushed to the right edge.

          `accessories` is how most of the store puts a count, a date or a
          state on a row, and none of it was drawn: an extension listing pull
          requests showed the titles and nothing about any of them. The spacer
          rather than `justify-content`, so a row with no accessories is laid
          out identically to one with three.
        -->
        <!--
          Nothing but the icon and the title once a detail pane is open.

          The rows are a third of a window then, and an icon, a title, a
          subtitle and two accessories do not fit in it: the last pill was cut
          in half against the boundary, which reads as a rendering fault rather
          than as a narrow column. The subtitle and the accessories are exactly
          what the pane beside them is for, so there is nothing to lose by
          letting it carry them.
        -->
        {#if !detail && accessoriesOf(row.node).length}
          <span class="spacer"></span>
          <span class="accessories">
            {#each accessoriesOf(row.node) as accessory, n (n)}
              <span class="accessory" class:tagged={accessory.tag !== undefined}>
                <!-- The tooltip is a whole span rather than an attribute on
                     the pieces, so one accessory is one thing to hover. -->
                {#if accessory.tooltip}
                  <span class="hoverable" use:hint={accessory.tooltip}>
                    {#if accessory.icon}<ExtIcon icon={accessory.icon} small />{/if}
                    {accessory.tag ?? accessory.text ?? ""}
                  </span>
                {:else}
                  {#if accessory.icon}<ExtIcon icon={accessory.icon} small />{/if}
                  {#if accessory.tag !== undefined}
                    <span
                      class="tag"
                      style={accessory.tint ? `color: ${accessory.tint}` : undefined}
                    >{accessory.tag}</span>
                  {:else if accessory.text}
                    <span class="accessory-text">{accessory.text}</span>
                  {/if}
                {/if}
              </span>
            {/each}
          </span>
        {/if}
      </div>
    {/if}
  {/each}

  <Instead tone={saying.tone} headline={saying.headline} hint={saying.hint} />
</div>

  {#if detail}
    <DetailPane {tree} node={detail} {version} beside />
  {/if}
</div>

<style>
  /*
   * The rows, and the pane beside them.
   *
   * `display: contents` while nothing is showing, so a list with no detail
   * pane is laid out by whatever contains it exactly as it was before this
   * wrapper existed. A div that is always a flex row would have changed the
   * height of every extension list in the launcher to fix one of them.
   */
  .split {
    display: contents;
  }

  .split.showing {
    display: flex;
    flex: 1;
    min-height: 0;
  }

  /*
   * Raycast gives the rows about a third and the reading half the rest, which
   * is the split a title column and a page of prose want.
   *
   * `overflow-x: hidden` because the rows are a column now rather than the
   * width of the window, and a row that is too wide for it must be cut off
   * rather than push a scrollbar under the list. A list you can drag sideways
   * is a list whose highlight goes off the edge.
   */
  .split.showing :global(.sill-list) {
    flex: 0 0 38%;
    min-width: 0;
    overflow-x: hidden;
  }

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
    /*
     * Allowed to shrink, and it has to be.
     *
     * With `flex: none` the title held its full width and pushed the
     * accessories off the right edge of the row, so a list narrowed by a
     * detail pane beside it showed half a pill hanging over the boundary. The
     * cap still holds: this is what a title does when there is room, and
     * ellipsis is what it does when there is not.
     */
    flex: 0 1 auto;
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

  /* What pushes the accessories to the right edge without changing how a row
     with none of them is laid out. */
  .spacer {
    flex: 1;
    min-width: var(--space-2);
  }

  /*
   * Allowed to shrink, and to lose its last accessory rather than the title.
   *
   * A row narrowed by a detail pane beside it has less room than the accessory
   * an extension wrote for a full-width window, and something has to give. The
   * title is what somebody is reading down, so the count on the right is what
   * goes.
   */
  .accessories {
    flex: 0 1 auto;
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-width: 0;
    overflow: hidden;
  }

  .accessory,
  .hoverable {
    display: inline-flex;
    align-items: center;
    gap: var(--space-half);
    color: var(--text-3);
    font-size: var(--text-meta);
    line-height: var(--line-meta);
    white-space: nowrap;
  }

  .accessory-text {
    color: var(--text-3);
  }

  /*
   * A tag accessory is a pill, and the same pill the metadata panel draws.
   *
   * The wash is mixed from the pill's own colour, so an untinted tag is the
   * ordinary fill and a tinted one is that shape in the extension's colour
   * rather than a second design for the coloured case.
   */
  .tag {
    padding: 0 var(--space-2);
    border-radius: var(--radius-pill);
    background-color: color-mix(in srgb, currentColor 16%, transparent);
    color: var(--text-2);
  }
</style>

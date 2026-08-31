<script lang="ts">
  /**
   * An answer, drawn.
   *
   * Every node is an element written here. Nothing is ever handed to the
   * document as a string of HTML, so there is no sanitiser and nothing for one
   * to miss: a model that has read a page trying to write markup produces
   * text, because text is the only thing the tree can hold.
   *
   * The look follows the one already worked out for Dosage's chat, which had
   * the benefit of being read every day for months. Two of its decisions carry
   * the rest: a hairline above every heading but the first, which chunks a
   * long reply into passages instead of one wall, and a table of four or more
   * columns becoming a card per row, because no amount of styling fits one
   * into a narrow window and sideways scrolling only hides the last column.
   */
  import { blocksOf, type Block } from "$lib/markdown";
  import Spans from "./Spans.svelte";
  import CodeBlock from "./CodeBlock.svelte";
  // By name rather than `<svelte:self>`, which Svelte 5 retired. A quote holds
  // anything a document holds, including another quote.
  import Self from "./Markdown.svelte";

  interface Props {
    /** The answer as it arrived. */
    text?: string;
    /**
     * Already parsed, for a quote drawing what is inside it.
     *
     * Either this or the text. Passing blocks rather than the source they came
     * from is what stops a nested quote being parsed once per level.
     */
    blocks?: Block[];
  }

  let { text, blocks }: Props = $props();

  const nodes = $derived(blocks ?? blocksOf(text ?? ""));

  /** How far a nested item sits in. Bounded, so a runaway list stays legible. */
  function indent(depth: number): string {
    return `${Math.min(depth, 4) * 14}px`;
  }

  /**
   * How many columns a table has, counted from its widest row.
   *
   * The header alone is not enough: a model writes a header of three and a row
   * of five often enough that trusting the first line puts the extra columns
   * off the edge.
   */
  function columns(block: Extract<Block, { kind: "table" }>): number {
    return Math.max(block.head.length, ...block.rows.map((row) => row.length), 0);
  }
</script>

{#each nodes as block, at (at)}
  {#if block.kind === "paragraph"}
    <p><Spans spans={block.spans} /></p>
  {:else if block.kind === "heading"}
    {#if block.level <= 2}
      <h3 class="rule h1"><Spans spans={block.spans} /></h3>
    {:else if block.level === 3}
      <h4 class="rule h2"><Spans spans={block.spans} /></h4>
    {:else}
      <h5 class="rule h3"><Spans spans={block.spans} /></h5>
    {/if}
  {:else if block.kind === "code"}
    <CodeBlock language={block.language} text={block.text} />
  {:else if block.kind === "rule"}
    <hr />
  {:else if block.kind === "quote"}
    <blockquote><Self blocks={block.blocks} /></blockquote>
  {:else if block.kind === "list"}
    {#if block.ordered}
      <ol class="list" start={block.start}>
        {#each block.items as item, n (n)}
          <li style="margin-left: {indent(item.depth)}"><Spans spans={item.spans} /></li>
        {/each}
      </ol>
    {:else}
      <ul class="list">
        {#each block.items as item, n (n)}
          <li class:task={item.done !== null} style="margin-left: {indent(item.depth)}">
            {#if item.done !== null}
              <!-- Drawn rather than a real checkbox: it reports what the
                   answer said, and nothing changes when it is pressed. -->
              <span class="box" class:done={item.done} aria-hidden="true"></span>
            {/if}
            <Spans spans={item.spans} />
          </li>
        {/each}
      </ul>
    {/if}
  {:else if block.kind === "table"}
    {#if columns(block) >= 4}
      <!-- One card per row. Four columns cannot fit this window whatever is
           done to them, and a sideways scroller just hides the last one. -->
      <div class="stack">
        {#each block.rows as row, n (n)}
          <div class="record">
            <p class="lead"><Spans spans={row[0] ?? []} /></p>
            <dl>
              {#each row.slice(1) as cell, c (c)}
                <div class="pair">
                  <dt>{block.head[c + 1] ? "" : ""}<Spans spans={block.head[c + 1] ?? []} /></dt>
                  <dd><Spans spans={cell} /></dd>
                </div>
              {/each}
            </dl>
          </div>
        {/each}
      </div>
    {:else}
      <div class="scroller sill-scrolls">
        <table>
          <thead>
            <tr>
              {#each block.head as cell, n (n)}
                <th><Spans spans={cell} /></th>
              {/each}
            </tr>
          </thead>
          <tbody>
            {#each block.rows as row, n (n)}
              <tr>
                {#each row as cell, c (c)}
                  <td><Spans spans={cell} /></td>
                {/each}
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    {/if}
  {/if}
{/each}

<style>
  p {
    margin: 0 0 var(--space-3);
    line-height: 1.65;
    overflow-wrap: anywhere;
  }

  p:last-child,
  .list:last-child,
  blockquote:last-child,
  .scroller:last-child,
  .stack:last-child {
    margin-bottom: 0;
  }

  /*
   * A hairline above every heading but the first.
   *
   * The separation the prose was missing. A long answer with three headings in
   * it reads as three passages rather than as one wall, and it costs a rule
   * rather than a size step, which is what keeps the type scale small enough
   * to sit inside a conversation.
   */
  .rule {
    margin: var(--space-4) 0 var(--space-2);
    padding-top: var(--space-3);
    border-top: 1px solid var(--hairline);
    font-weight: var(--weight-strong);
    line-height: 1.35;
    color: var(--text-1);
  }

  .rule:first-child {
    margin-top: 0;
    padding-top: 0;
    border-top: 0;
  }

  /*
   * Three steps, all of them small.
   *
   * A model writes `#` for the top of what it is saying, which inside a
   * conversation is a lead rather than a page title. `--text-title` is 20px
   * against 13px of body and reads as a heading that wandered in from another
   * window, so the largest step here is 15px.
   */
  .h1 {
    font-size: var(--text-heading);
  }

  .h2 {
    font-size: var(--text-body);
  }

  .h3 {
    font-size: var(--text-label);
    color: var(--text-2);
    letter-spacing: 0.12em;
    text-transform: uppercase;
  }

  .list {
    margin: 0 0 var(--space-3);
    padding-left: var(--space-5);
    line-height: 1.65;
  }

  li {
    margin-bottom: var(--space-1);
    overflow-wrap: anywhere;
  }

  /*
   * The marker takes the accent.
   *
   * The one place in an answer where the accent is structure rather than
   * decoration: it is what makes a list read as a list at a glance in a
   * paragraph of the same weight, and it is a single considered use in one
   * component rather than the scattered alphas the token system exists to
   * stop.
   */
  li::marker {
    color: var(--accent);
  }

  /* A checkbox item carries its own mark, so it does not want a bullet too. */
  .task {
    list-style: none;
    margin-left: calc(var(--space-5) * -1);
    display: flex;
    align-items: baseline;
    gap: var(--space-2);
  }

  .box {
    position: relative;
    top: 1px;
    width: 12px;
    height: 12px;
    flex: none;
    border-radius: 3px;
    box-shadow: inset 0 0 0 1px var(--hairline-strong);
  }

  .box.done {
    background: var(--accent);
    box-shadow: none;
  }

  .box.done::after {
    content: "";
    position: absolute;
    inset: 2px 3px 3px;
    border-left: 1.5px solid var(--core-background);
    border-bottom: 1.5px solid var(--core-background);
    transform: rotate(-45deg);
  }

  blockquote {
    margin: 0 0 var(--space-3);
    padding: var(--space-2) var(--space-3);
    border-left: 2px solid var(--accent-line);
    border-radius: 0 var(--radius-md) var(--radius-md) 0;
    background: var(--fill-1);
    color: var(--text-2);
  }

  hr {
    margin: var(--space-4) 0;
    border: 0;
    border-top: 1px solid var(--hairline);
  }

  /* Its own scroller, so a wide table never makes the conversation move. */
  .scroller {
    margin: 0 0 var(--space-3);
    overflow-x: auto;
    border-radius: var(--radius-md);
    background: var(--fill-1);
    box-shadow: inset 0 0 0 1px var(--hairline);
  }

  table {
    width: 100%;
    border-collapse: collapse;
    font-size: var(--text-meta);
  }

  th {
    padding: var(--space-2) var(--space-3);
    background: var(--fill-2);
    color: var(--text-3);
    font-size: var(--text-micro);
    font-weight: var(--weight-strong);
    letter-spacing: 0.08em;
    text-align: left;
    text-transform: uppercase;
    white-space: nowrap;
  }

  td {
    padding: var(--space-2) var(--space-3);
    border-top: 1px solid var(--hairline);
    color: var(--text-2);
    vertical-align: top;
  }

  /* The first column is what the row is about, so it reads as the subject. */
  td:first-child {
    color: var(--text-1);
    font-weight: var(--weight-medium);
  }

  .stack {
    margin: 0 0 var(--space-3);
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .record {
    padding: var(--space-3);
    border-radius: var(--radius-md);
    background: var(--fill-1);
    box-shadow: inset 0 0 0 1px var(--hairline);
  }

  .lead {
    margin: 0 0 var(--space-2);
    color: var(--text-1);
    font-weight: var(--weight-strong);
  }

  dl {
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }

  .pair {
    display: flex;
    gap: var(--space-3);
  }

  dt {
    flex: none;
    width: 8ch;
    color: var(--text-3);
    font-size: var(--text-micro);
    font-weight: var(--weight-strong);
    letter-spacing: 0.08em;
    text-transform: uppercase;
    padding-top: 1px;
  }

  dd {
    margin: 0;
    min-width: 0;
    flex: 1;
    color: var(--text-2);
    line-height: 1.55;
  }
</style>

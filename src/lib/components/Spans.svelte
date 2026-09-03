<script lang="ts">
  /**
   * One run of inline text, drawn.
   *
   * Recursive, because emphasis nests. Every branch is an element written
   * here, so nothing reaches the document as a string of markup.
   *
   * A single newline is a line break, which is `remarkBreaks` in the renderer
   * this follows. Strict markdown folds one into a space, and models do not
   * write that way: an address, a list of names, a set of steps written
   * without bullets all arrive as single newlines and all read as one run-on
   * sentence without this.
   */
  import type { Span } from "$lib/markdown";
  import Self from "./Spans.svelte";

  interface Props {
    spans: Span[];
  }

  let { spans }: Props = $props();

  /** Text split at its newlines, so the drawing can put a break between. */
  function runs(text: string): string[] {
    return text.split("\n");
  }
</script>

{#each spans as span, at (at)}{#if span.kind === "text"}{#each runs(span.text) as run, line (line)}{#if line > 0}<br
        />{/if}{run}{/each}{:else if span.kind === "code"}<code>{span.text}</code
    >{:else if span.kind === "strong"}<strong><Self spans={span.spans} /></strong
    >{:else if span.kind === "em"}<em><Self spans={span.spans} /></em
    >{:else if span.kind === "strike"}<s><Self spans={span.spans} /></s
    >{:else if span.kind === "link"}<a
      href={span.href}
      target="_blank"
      rel="noreferrer noopener">{#if span.spans.length}<Self
          spans={span.spans}
        />{:else}{span.href}{/if}</a
    >{/if}{/each}

<style>
  /*
   * Whitespace matters in this file.
   *
   * The markup above is written with no gaps between its branches on purpose.
   * Svelte keeps the whitespace inside an `{#each}`, so a newline between two
   * spans becomes a space in the middle of a word. Hence the closing brackets
   * hanging at the start of lines, which is ugly and is exactly the point.
   */
  code {
    padding: 0.12em 0.36em;
    border-radius: var(--radius-sm);
    background: var(--fill-2);
    box-shadow: var(--ring);
    font-family: var(--font-mono);
    font-size: 0.9em;
    overflow-wrap: anywhere;
  }

  strong {
    font-weight: var(--weight-strong);
    color: var(--text-1);
  }

  em {
    font-style: italic;
  }

  s {
    color: var(--text-3);
  }

  /*
   * `anywhere`, because a bare address is one word.
   *
   * Without it a single long link decides the width of the whole conversation
   * and everything beside it scrolls sideways to match.
   */
  a {
    color: var(--accent);
    text-decoration: underline;
    text-decoration-color: var(--accent-line);
    text-underline-offset: 2px;
    overflow-wrap: anywhere;
  }

  a:hover {
    text-decoration-color: var(--accent);
  }
</style>

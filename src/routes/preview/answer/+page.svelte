<script lang="ts">
  /**
   * A development route. Reachable with `npm run dev`, never linked from the
   * app, and of no use to anyone running Sill rather than working on it.
   *
   * An answer, rendered outside Tauri so the markdown can be judged whole.
   * Waiting for a model to write one of these takes half a minute and produces
   * whatever it felt like producing; this holds still and covers every block
   * the parser knows, which is the only way to see them all at once.
   */
  import "$lib/theme/theme.css";
  import Markdown from "$lib/components/Markdown.svelte";

  const ANSWER = `Two things are eating your battery. Here is what I found and what to do.

# Steps

1. Quit **Everything.exe**, which is holding 415 MB and indexing
2. Close the Chrome windows on the second display

Run this to see the rest:

\`\`\`sh
# top ten by memory
ps | sort -k2 -n -r | head -10
\`\`\`

## Notes

- **Everything** is a hard dependency of file search, so quitting it turns that off until it starts again
- Use \`Get-Process\` if you would rather not install anything
- [The docs for it](https://example.com/docs) explain the indexing schedule
- ~~Restarting the machine~~ is not necessary

### What each one costs

| Program | Memory |
| --- | --- |
| Everything.exe | 415 MB |
| chrome.exe | 1.2 GB |

| Program | Memory | CPU | Since |
| --- | --- | --- | --- |
| Everything.exe | 415 MB | 2.1% | 09:14 |
| chrome.exe | 1.2 GB | 8.4% | 07:02 |

> Quitting Everything while it is mid-index means it starts again from the
> beginning, which is another twenty minutes.

- [x] Checked what is running
- [x] Read the power state
- [ ] Quit anything

---

Ask again in a few minutes and I will tell you whether it worked.`;

  const SHORT = `Red, blue and yellow.`;

  const NESTED = `A list with depth:

- One
  - Under one
    - Deeper still
- Two

And a quote holding a list:

> Be careful with these:
>
> - The first
> - The second`;
</script>

<div class="page">
  <h1>Answers</h1>
  <p class="note">Every block the parser knows, drawn by the real components.</p>

  <div class="chat">
    <article class="turn asked"><p>what is eating my battery</p></article>
    <article class="turn said md"><Markdown text={ANSWER} /></article>

    <article class="turn asked"><p>name a colour, one word</p></article>
    <article class="turn said md"><Markdown text={SHORT} /></article>

    <article class="turn asked"><p>show me nesting</p></article>
    <article class="turn said md"><Markdown text={NESTED} /></article>
  </div>
</div>

<style>
  /*
   * The launcher sizes its own body to a fixed window and hides the overflow,
   * which is right there and wrong here: outside Tauri that clips the page at
   * the height the launcher happens to be.
   */
  :global(html),
  :global(body) {
    width: auto;
    height: auto;
    min-height: 100%;
    overflow: visible;
  }

  :global(body) {
    margin: 0;
    background: var(--core-background);
    color: var(--text-1);
    font-family: var(--font);
  }

  .page {
    max-width: 760px;
    margin: 0 auto;
    padding: var(--space-6) var(--space-4);
  }

  h1 {
    margin: 0;
    font-size: var(--text-title);
  }

  .note {
    margin: var(--space-1) 0 var(--space-5);
    color: var(--text-3);
    font-size: var(--text-meta);
  }

  /* Copied from the launcher, because that is what is being judged. */
  .chat {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
    padding: var(--space-4) var(--space-4) var(--space-5);
    border-radius: var(--radius-lg);
    background: var(--core-secondary-background);
  }

  .turn {
    font-size: var(--text-body);
  }

  .asked {
    align-self: flex-end;
    max-width: 78%;
    padding: var(--space-2) var(--space-3);
    border-radius: var(--radius-lg) var(--radius-lg) var(--radius-sm) var(--radius-lg);
    background: var(--accent-fill);
    box-shadow: inset 0 0 0 1px var(--accent-line);
  }

  .asked p {
    margin: 0;
    color: var(--text-1);
    line-height: 1.55;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }

  .said {
    align-self: flex-start;
    max-width: 68ch;
    width: 100%;
    color: var(--text-1);
  }
</style>

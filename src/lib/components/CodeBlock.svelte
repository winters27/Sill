<script lang="ts">
  /**
   * A fenced block, with the one thing anybody wants from one.
   *
   * Copying is the entire reason a command in an answer is useful, and
   * selecting text in a launcher window that closes when it loses focus is not
   * a thing anybody manages twice. So the button is always there rather than
   * on hover: a control that appears when the pointer arrives is a control
   * somebody has to already know about.
   */
  import { writeText } from "@tauri-apps/plugin-clipboard-manager";

  interface Props {
    language: string;
    text: string;
  }

  let { language, text }: Props = $props();

  let copied = $state(false);
  let clearing: ReturnType<typeof setTimeout> | undefined;

  async function copy() {
    try {
      await writeText(text);
      copied = true;
      clearTimeout(clearing);
      // Long enough to be read, short enough that the button is ready again
      // before somebody wants it.
      clearing = setTimeout(() => (copied = false), 1400);
    } catch {
      // The clipboard refusing is rare and the block is still on screen to be
      // selected. Saying nothing beats a toast over a two line answer.
    }
  }

  /** Lines, so the block can be numbered and wrapped one line at a time. */
  const lines = $derived(text.split("\n"));
</script>

<div class="block">
  <div class="bar">
    <span class="language">{language || "text"}</span>
    <button onclick={copy} class:done={copied}>{copied ? "Copied" : "Copy"}</button>
  </div>

  <!-- Its own scroller, so a long line never makes the conversation scroll
       sideways. -->
  <pre class="sill-scrolls"><code>{#each lines as line, at (at)}<span class="line">{line}</span>{"\n"}{/each}</code></pre>
</div>

<style>
  /*
   * As wide as the code, and no wider.
   *
   * A three line shell snippet stretched across a seventy character column is
   * mostly empty box, and the Copy button ends up a screen away from the
   * language it belongs to. `max-width` keeps a long line scrolling inside
   * rather than pushing the conversation sideways.
   */
  .block {
    width: fit-content;
    min-width: 16ch;
    max-width: 100%;
    margin: 0 0 var(--space-3);
    border-radius: var(--radius-md);
    background: var(--fill-1);
    box-shadow: var(--ring);
    overflow: hidden;
  }

  .block:last-child {
    margin-bottom: 0;
  }

  .bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    padding: var(--space-1) var(--space-1) var(--space-1) var(--space-3);
    border-bottom: 1px solid var(--hairline);
  }

  .language {
    color: var(--text-3);
    font-family: var(--font-mono);
    font-size: var(--text-micro);
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }

  button {
    padding: var(--space-half) var(--space-2);
    border: 0;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--text-3);
    font: inherit;
    font-size: var(--text-micro);
    cursor: pointer;
    transition:
      color var(--motion-state) var(--ease),
      background-color var(--motion-state) var(--ease);
  }

  button:hover {
    background: var(--fill-2);
    color: var(--text-1);
  }

  button.done {
    color: var(--accent);
  }

  button:focus-visible {
    outline: none;
    box-shadow: var(--ring-accent);
  }

  pre {
    margin: 0;
    padding: var(--space-3);
    overflow-x: auto;
  }

  code {
    font-family: var(--font-mono);
    font-size: var(--text-meta);
    line-height: 1.55;
    color: var(--text-1);
  }

  /* Each line is its own element so nothing wraps, which is what keeps a
     command copyable by eye as well as by button. */
  .line {
    display: inline-block;
    white-space: pre;
  }
</style>

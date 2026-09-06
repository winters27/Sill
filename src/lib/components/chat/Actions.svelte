<script lang="ts">
  /**
   * Copy and ask again, on the answer they belong to.
   *
   * Quiet until the answer is hovered, because they are about the answer
   * rather than part of it, and a row of controls under every reply is a
   * conversation with furniture in it. The row keeps its height while
   * hidden, so hovering does not move the paragraph above.
   */
  import { writeText } from "@tauri-apps/plugin-clipboard-manager";

  interface Props {
    text: string;
    /** Ask the same question again. Only offered on the last answer. */
    onagain?: () => void;
    /** Whether a question is already in flight. */
    busy?: boolean;
  }

  let { text, onagain, busy = false }: Props = $props();

  let copied = $state(false);
  let flash = 0;

  async function copy() {
    try {
      await writeText(text);
      copied = true;
      window.clearTimeout(flash);
      flash = window.setTimeout(() => (copied = false), 1400);
    } catch {
      // The clipboard refusing is rare and the answer is still on screen to
      // be selected. Saying nothing beats a message over a two line reply.
    }
  }
</script>

<div class="afters">
  <button onclick={() => void copy()}>{copied ? "Copied" : "Copy"}</button>
  {#if onagain}
    <button onclick={() => onagain?.()} disabled={busy}>Again</button>
  {/if}
</div>

<style>
  .afters {
    display: flex;
    gap: var(--space-1);
    margin-top: var(--space-1);
    opacity: 0;
    transition: opacity var(--motion-state) var(--ease);
  }

  :global(.said:hover) .afters,
  .afters:focus-within {
    opacity: 1;
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

  button:hover:not(:disabled) {
    background: var(--fill-2);
    color: var(--text-1);
  }

  button:disabled {
    opacity: var(--opacity-disabled);
    cursor: default;
  }
</style>

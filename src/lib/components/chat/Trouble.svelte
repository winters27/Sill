<script lang="ts">
  /**
   * What went wrong, said in proportion.
   *
   * A limit is the service saying come back later: a plain sentence, no
   * alarm colour, nothing to press. Everything else is a failure worth a
   * card and a Try again, because the question is still there and asking it
   * once more is usually what fixes a dropped connection.
   */
  import { tier } from "$lib/chat/trouble";

  interface Props {
    why: string;
    /** Ask the last question again. Absent when there is nothing to re-ask. */
    onagain?: () => void;
    busy?: boolean;
  }

  let { why, onagain, busy = false }: Props = $props();

  const kind = $derived(tier(why));
</script>

{#if kind === "limit"}
  <p class="calm" role="status">{why}</p>
{:else}
  <div class="failed" role="alert">
    <p class="what">The assistant hit a problem</p>
    <p class="why">{why}</p>
    {#if onagain}
      <button onclick={() => onagain?.()} disabled={busy}>Try again</button>
    {/if}
  </div>
{/if}

<style>
  .calm {
    align-self: flex-start;
    max-width: 62ch;
    margin: 0;
    color: var(--text-2);
    font-size: var(--text-meta);
    line-height: 1.5;
  }

  .failed {
    align-self: flex-start;
    max-width: 62ch;
    width: 100%;
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    padding: var(--space-3);
    border-radius: var(--radius-lg);
    background: var(--fill-1);
    box-shadow: var(--ring);
  }

  .what {
    margin: 0;
    color: var(--danger);
    font-size: var(--text-meta);
    font-weight: var(--weight-strong);
  }

  .why {
    margin: 0;
    color: var(--text-2);
    font-size: var(--text-meta);
    line-height: 1.5;
    overflow-wrap: anywhere;
  }

  button {
    align-self: flex-start;
    margin-top: var(--space-1);
    padding: var(--space-1) var(--space-3);
    border: 0;
    border-radius: var(--radius-sm);
    background: var(--fill-2);
    color: var(--text-1);
    font: inherit;
    font-size: var(--text-meta);
    cursor: pointer;
    transition: background-color var(--motion-state) var(--ease);
  }

  button:hover:not(:disabled) {
    background: var(--fill-3);
  }

  button:disabled {
    opacity: var(--opacity-disabled);
    cursor: default;
  }
</style>

<script lang="ts">
  /**
   * What the model looked at, above the answer it produced.
   *
   * Shared by the launcher and the chat window, which drew the same list twice
   * with the same words in two places. One of them was going to grow a step
   * the other did not have.
   *
   * ## Open while it is working, folded once it is done
   *
   * A turn in flight shows every step as it happens, because that is the only
   * thing on screen saying anything is happening. A finished turn folds to one
   * line, because five lines of grey text above every answer is provenance
   * that has become furniture. The same decision Dosage's chat arrived at
   * after months of being read every day.
   */
  import type { AiStep } from "$lib/exthost/commands";

  interface Props {
    steps: AiStep[];
    /** Whether the turn these belong to is still being written. */
    live?: boolean;
  }

  let { steps, live = false }: Props = $props();

  /** Set once somebody presses it, and then it decides rather than `live`. */
  let opened = $state<boolean | null>(null);

  const open = $derived(opened ?? live);

  /**
   * What each step did, in words.
   *
   * A table rather than the tool's own name, because the names are written for
   * a model and read like an API. Anything unknown falls back to the name
   * rather than to nothing: a tool added later should read oddly rather than
   * vanish, which is the mistake this codebase has made four times with lists
   * of modes.
   */
  const DID: Record<string, string> = {
    search_sill: "Searched what is on this machine for",
    find_files: "Looked for files called",
    read_file: "Read",
    list_directory: "Looked inside",
    read_clipboard: "Read what you have copied",
    list_windows: "Looked at what is open",
    system_state: "Checked how this machine is set",
    read_selection: "Read what was selected",
    read_screen: "Read what is on screen",
    what_can_be_done: "Worked out what can be done to",
    run_action: "Acted on",
  };

  function didWhat(step: AiStep): string {
    const said = DID[step.tool] ?? step.tool;
    return step.subject ? `${said} ${step.subject}` : said;
  }

  /** The first step, and how many others there were. */
  const summary = $derived.by(() => {
    if (steps.length === 0) return "";
    const first = didWhat(steps[0]);
    return steps.length === 1 ? first : `${first} and ${steps.length - 1} more`;
  });
</script>

{#if steps.length}
  <div class="steps">
    <button
      class="head"
      onclick={() => (opened = !open)}
      aria-expanded={open}
      title={open ? "Hide what it looked at" : "Show what it looked at"}
    >
      <span class="pip" class:working={live} aria-hidden="true"></span>
      <span class="said">{open ? "What it looked at" : summary}</span>
      <svg
        class="chevron"
        class:up={open}
        width="10"
        height="10"
        viewBox="0 0 10 10"
        fill="none"
        aria-hidden="true"
      >
        <path d="M2 3.5L5 6.5l3-3" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" />
      </svg>
    </button>

    {#if open}
      <div class="list">
        {#each steps as step, at (at)}
          <p class="step">{didWhat(step)}</p>
        {/each}
      </div>
    {/if}
  </div>
{/if}

<style>
  .steps {
    align-self: flex-start;
    max-width: 68ch;
  }

  .head {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: 0;
    border: 0;
    background: transparent;
    color: var(--text-3);
    font: inherit;
    font-size: var(--text-meta);
    text-align: left;
    cursor: pointer;
    transition: color 0.15s var(--ease);
  }

  .head:hover {
    color: var(--text-2);
  }

  .head:focus-visible {
    outline: none;
    color: var(--text-1);
  }

  .said {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .pip {
    width: 4px;
    height: 4px;
    flex: none;
    border-radius: 50%;
    background: var(--accent);
    opacity: 0.7;
  }

  /* Something is happening, said without a spinner. */
  .working {
    animation: pulse 1.4s ease-in-out infinite;
  }

  @keyframes pulse {
    0%,
    100% {
      opacity: 0.25;
    }
    50% {
      opacity: 1;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .working {
      animation: none;
      opacity: 1;
    }
  }

  .chevron {
    flex: none;
    transition: transform 0.15s var(--ease);
  }

  .up {
    transform: rotate(180deg);
  }

  .list {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    /* Lined up under the words above rather than under the mark, so the
       summary and the steps it summarises read as one column. */
    margin: var(--space-1) 0 0 calc(4px + var(--space-2));
  }

  .step {
    margin: 0;
    color: var(--text-3);
    font-size: var(--text-meta);
    line-height: 1.5;
    overflow-wrap: anywhere;
  }
</style>

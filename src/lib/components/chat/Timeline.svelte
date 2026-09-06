<script lang="ts">
  /**
   * What the model reached for, drawn where it happened in the answer.
   *
   * ## Open while it is working, folded once it is done
   *
   * A turn in flight shows every step as it happens, because that is the
   * only thing on screen saying anything is happening. A finished turn folds
   * to one line, because five lines of grey text above every answer is
   * provenance that has become furniture. Dosage's chat arrived at the same
   * decision after months of being read every day, and this is a port of it.
   *
   * It stays after the answer arrives: knowing that a question about your
   * machine was answered by reading your clipboard is part of the answer.
   *
   * ## Each step is a sentence
   *
   * One line per tool with what it was used on, because ten lookups that all
   * read "Searched" read as a stutter. Consecutive lines that would read the
   * same fold into one with a count.
   */
  import { drawer } from "$lib/motion";
  import { hint } from "$lib/hint";
  import type { StepPart } from "$lib/chat/parts";
  import { describe, verbFor } from "$lib/chat/verbs";
  import StepIcon from "./StepIcon.svelte";

  interface Props {
    steps: StepPart[];
    /** Whether the turn these belong to is still being written. */
    live?: boolean;
  }

  let { steps, live = false }: Props = $props();

  /** Set once somebody presses it, and then it decides rather than `live`. */
  let opened = $state<boolean | null>(null);
  const open = $derived(opened ?? live);

  /**
   * A step is running only on a live turn and only until its result.
   *
   * A saved conversation reopened later has steps that never got a result,
   * and reading those as running left old answers spinning forever.
   */
  function running(step: StepPart): boolean {
    return live && step.ok === undefined;
  }

  const working = $derived(steps.some(running));

  interface Row {
    step: StepPart;
    label: string;
    count: number;
    running: boolean;
  }

  /** The lines to draw, with repeats folded into one. */
  const rows = $derived.by(() => {
    const out: Row[] = [];
    for (const step of steps) {
      const isRunning = running(step);
      const label = describe(step, isRunning);
      const last = out[out.length - 1];
      if (last && last.label === label && !last.running && !isRunning) {
        last.count += 1;
      } else {
        out.push({ step, label, count: 1, running: isRunning });
      }
    }
    return out;
  });

  const summary = $derived.by(() => {
    if (steps.length === 0) return "";
    const first = describe(steps[0], false);
    return steps.length === 1 ? first : `${first} +${steps.length - 1} more`;
  });

  const heading = $derived(open ? (working ? "Working" : "What it looked at") : summary);
</script>

{#if steps.length}
  <div class="timeline">
    <button
      class="head"
      onclick={() => (opened = !open)}
      aria-expanded={open}
      use:hint={open ? "Hide what it looked at" : "Show what it looked at"}
    >
      <span class="pip" class:working aria-hidden="true"></span>
      <span class="said">{heading}</span>
      <svg class="chevron" class:up={open} width="10" height="10" viewBox="0 0 10 10" fill="none" aria-hidden="true">
        <path d="M2 3.5L5 6.5l3-3" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" />
      </svg>
    </button>

    {#if open}
      <ol class="rows" transition:drawer>
        {#each rows as row, at (row.step.id || at)}
          <li class="row" class:running={row.running} class:failed={row.step.ok === false}>
            <span class="rail" aria-hidden="true">
              <span class="node"><StepIcon name={verbFor(row.step.tool).icon} /></span>
              {#if at < rows.length - 1}<span class="line"></span>{/if}
            </span>
            <span class="label">
              {row.label}{#if row.count > 1}<span class="count"> x{row.count}</span>{/if}
              {#if row.step.ok === false}<span class="note" use:hint={"This did not work"}> did not work</span>{/if}
            </span>
          </li>
        {/each}
      </ol>
    {/if}
  </div>
{/if}

<style>
  .timeline {
    align-self: flex-start;
    max-width: 68ch;
    width: 100%;
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
    transition: color var(--motion-state) var(--ease);
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
    animation: pulse var(--motion-pulse) ease-in-out infinite;
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

  .chevron {
    flex: none;
    transition: transform var(--motion-state) var(--ease);
  }

  .up {
    transform: rotate(180deg);
  }

  .rows {
    list-style: none;
    margin: var(--space-2) 0 0;
    padding: 0;
  }

  .row {
    display: flex;
    gap: var(--space-2);
    align-items: stretch;
  }

  /* The node and the hairline down to the next one. */
  .rail {
    display: flex;
    flex-direction: column;
    align-items: center;
    flex: none;
    width: var(--timeline-node);
  }

  .node {
    display: grid;
    place-items: center;
    flex: none;
    width: var(--timeline-node);
    height: var(--timeline-node);
    border-radius: 50%;
    background: var(--fill-2);
    color: var(--text-2);
    transition: color var(--motion-state) var(--ease);
  }

  .running .node {
    color: var(--accent);
    animation: pulse var(--motion-pulse) ease-in-out infinite;
  }

  .failed .node {
    color: var(--warning);
  }

  .line {
    flex: 1;
    width: 1px;
    min-height: var(--space-2);
    background: var(--timeline-line);
  }

  .label {
    padding: 0 0 var(--space-2);
    color: var(--text-3);
    font-size: var(--text-meta);
    line-height: var(--timeline-node);
    overflow-wrap: anywhere;
  }

  .running .label {
    color: var(--text-2);
  }

  .count,
  .note {
    color: var(--text-3);
  }

  .note {
    color: var(--warning);
  }

  @media (prefers-reduced-motion: reduce) {
    .working,
    .running .node {
      animation: none;
      opacity: 1;
    }
  }
</style>

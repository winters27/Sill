<!--
  The first summon on a machine Sill has not run on before.

  Every sentence on this page comes from `welcome` in Rust, which builds it
  from what the machine actually answered rather than from what the settings
  file asks for. The one that matters is the first: it names the key that opens
  Sill, and the key in the settings file is the key that was **requested**.
  On the machine this was written on those two have disagreed at every start
  for weeks, so a page that wrote "Press Alt+Space" here would open somebody's
  first minute with Sill by telling them to press a key that does nothing.

  That is also why `verify:source` refuses a chord typed into this file, the
  same rule the keyboard reference lives under.
-->
<script lang="ts">
  import type { Welcome } from "$lib/exthost/commands";
  import { LISTBOX, optionId } from "$lib/results";

  interface Props {
    said: Welcome;
    selected: number;
    onselect: (index: number) => void;
    onrun: (index: number) => void;
  }

  let { said, selected, onselect, onrun }: Props = $props();

  /**
   * True while the pointer is what moved the selection.
   *
   * The same guard the root list carries, and for the same reason: scrolling a
   * row fully into view pulls the next one under a stationary cursor, which
   * selects it, which scrolls again. Plain rather than `$state`, because the
   * effect reads it without wanting to run when it changes.
   */
  let byPointer = false;
  let viewport = $state<HTMLDivElement | null>(null);

  /*
   * The selected row, kept where it can be seen.
   *
   * Five rows under two paragraphs is taller than the launcher, so arrowing to
   * the last one walks off the bottom without this.
   */
  $effect(() => {
    const at = selected;
    if (byPointer) {
      byPointer = false;
      return;
    }

    viewport
      ?.querySelector(`[data-step="${at}"]`)
      ?.scrollIntoView({ block: "nearest" });
  });
</script>

<div class="welcome sill-scrolls" bind:this={viewport}>
  <!--
    Marked when the key was refused, and marked in one place.

    The headline carries it rather than the headline and the body both: the
    same fact said twice in the same colour reads as an error state around the
    whole page rather than as one sentence worth reading first.
  -->
  <section class="says" class:wrong={said.summonTaken}>
    <h2>{said.opening.headline}</h2>
    <p>{said.opening.body}</p>
  </section>

  <section class="says">
    <h2>{said.tray.headline}</h2>
    <p>{said.tray.body}</p>
  </section>

  <div id={LISTBOX} class="steps" role="listbox" tabindex="-1" aria-label="Set up Sill">
    {#each said.steps as step, index (step.id)}
      <div
        id={optionId(index)}
        data-step={index}
        class="step"
        class:selected={index === selected}
        role="option"
        aria-selected={index === selected}
        tabindex="-1"
        onmousemove={() => {
          if (index === selected) return;
          byPointer = true;
          onselect(index);
        }}
        onclick={() => onrun(index)}
        onkeydown={(event) => event.key === "Enter" && onrun(index)}
      >
        <span class="step-title">{step.title}</span>
        <span class="step-subtitle">{step.subtitle}</span>
      </div>
    {/each}
  </div>
</div>

<style>
  .welcome {
    overflow-y: auto;
    padding: var(--space-4) var(--space-4) var(--space-4);
  }

  /*
   * The two paragraphs above the rows.
   *
   * Prose rather than rows because neither is something to do. What opens Sill
   * and what the notification area icon is are facts somebody needs before the
   * rows below make sense, and a row that only tells you something swallows
   * Enter, which reads as the launcher having stopped responding.
   */
  .says {
    margin-bottom: var(--space-4);
  }

  .says h2 {
    margin: 0 0 var(--space-1);
    font-size: var(--text-body);
    font-weight: var(--weight-medium);
    color: var(--text-1);
  }

  .says p {
    margin: 0;
    max-width: 62ch;
    font-size: var(--text-meta);
    /* A ratio rather than a --line-* token, which states a row box in px so a
       list keeps its height when the interface face changes. This wraps, so
       what it wants is the space between two lines of prose. */
    line-height: 1.6;
    color: var(--text-3);
  }

  /*
   * The one coloured thing on the page, and only when the key was refused.
   *
   * The headline rather than the paragraph, matching `Instead`'s rule that
   * exactly one element carries the colour: a red heading over a red sentence
   * turns a key that needs changing into a screen that looks like data loss.
   */
  .says.wrong h2 {
    color: var(--danger);
  }

  .steps {
    display: flex;
    flex-direction: column;
    gap: var(--space-half);
    outline: none;
  }

  /*
   * Not `.sill-row`.
   *
   * That primitive is a fixed `--row-height` with one line of text that
   * ellipsises, which is right for a list of applications and wrong here: the
   * subtitle is the sentence that says what choosing the row will do, and a
   * row that hides it is a row somebody presses without knowing. Everything
   * else about it, the radius, the fill, the selected catch, is the same, so
   * it still reads as part of the same list system.
   */
  .step {
    display: flex;
    flex-direction: column;
    gap: var(--space-half);
    padding: var(--space-2) var(--space-3);
    border-radius: var(--radius-md);
    cursor: default;
    transition:
      background-color var(--motion-state) var(--ease),
      box-shadow var(--motion-state) var(--ease);
  }

  .step:hover:not(.selected) {
    background-color: var(--fill-1);
  }

  .step.selected {
    background-color: var(--accent-fill);
    box-shadow: var(--catch);
  }

  .step-title {
    font-size: var(--text-body);
    font-weight: var(--weight-body);
    color: var(--text-1);
  }

  .step-subtitle {
    max-width: 62ch;
    font-size: var(--text-meta);
    line-height: 1.6;
    color: var(--text-3);
  }
</style>

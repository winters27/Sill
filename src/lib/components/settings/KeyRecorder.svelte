<!--
  The one control that records a key.

  ## What it replaced

  Three recorders, one per section of the shortcuts panel, each with its own
  listener, its own idea of a chord and its own way of saying no. The global
  keys were caught by a window-level listener that never disarmed, so clicking
  a different row in the panel and pressing a key could rebind the summon key.
  None of them showed anything while modifiers were held, and none of them
  asked whether the key already did something elsewhere.

  ## What it does

  Draws the chord as keycaps. While recording it draws the modifiers held so
  far, dimmed, ahead of a pulsing cursor cap, so the person can see what they
  are building. On the key that finishes the chord it asks Rust what already
  runs on it: something in this recorder's own section is a refusal, said in
  place; something in another section is saved and mentioned, because a key
  that opens the switcher and also moves the selection is legal and worth
  knowing. Escape cancels, Backspace and Delete clear or reset according to
  what the row allows, and losing focus cancels, so nothing here is ever
  listening when this control is not the thing under the pointer.

  The chord grammar is `$lib/keys`; which keys are taken is Rust's answer.
  Nothing here runs when the control is idle.
-->
<script lang="ts">
  import Chord from "../Chord.svelte";
  import { hint } from "$lib/hint";
  import { chordFor, type Scope } from "$lib/keys";
  import { keyOwners, type KeyOwner } from "$lib/settings";

  interface Props {
    chord: string;
    scope: Scope;
    /** The section of the keyboard reference this key lives in. A key that
     *  already does something in the same section is refused. */
    section: string;
    /** Saves the chord. Awaited, so the panel can re-read what it resolved to. */
    onsave: (chord: string) => Promise<void>;
    /** Takes the key away. Absent for a key that cannot be off. */
    onclear?: () => Promise<void>;
    /** Puts the key back to what it shipped with. Backspace, where it exists. */
    onreset?: () => Promise<void>;
    /** Another action on the same list takes this chord first. */
    contested?: string;
    /** What the control says when there is no key. */
    placeholder?: string;
    ariaLabel?: string;
  }

  let {
    chord,
    scope,
    section,
    onsave,
    onclear,
    onreset,
    contested,
    placeholder = "Set a key",
    ariaLabel,
  }: Props = $props();

  let recording = $state(false);
  let saving = $state(false);
  /** The modifiers down right now, drawn ahead of the cursor. */
  let held = $state<string[]>([]);
  /** What the last press amounted to, said under the control. */
  let note = $state<{ text: string; refused: boolean; more?: string } | null>(null);
  let button = $state<HTMLButtonElement | null>(null);

  function start(): void {
    recording = true;
    held = [];
    note = null;
  }

  function stop(): void {
    recording = false;
    held = [];
  }

  function toggle(): void {
    if (recording) stop();
    else start();
  }

  /** What already runs on a chord, or nothing when Rust cannot be asked. */
  async function ownersOf(next: string): Promise<KeyOwner[]> {
    try {
      return await keyOwners(next);
    } catch {
      return [];
    }
  }

  async function commit(next: string, caution?: string): Promise<void> {
    if (next === chord) {
      stop();
      return;
    }

    saving = true;
    try {
      const owners = await ownersOf(next);
      const here = owners.find((owner) => owner.section === section);
      if (here) {
        // Refused, and still recording: the reason is on screen and the next
        // key can be tried without clicking again.
        note = { text: `Already ${here.does.charAt(0).toLowerCase()}${here.does.slice(1)} here`, refused: true };
        held = [];
        return;
      }

      await onsave(next);
      const elsewhere = owners[0];
      note = elsewhere
        ? { text: `Also ${elsewhere.does.charAt(0).toLowerCase()}${elsewhere.does.slice(1)} (${elsewhere.section})`, refused: false }
        : caution
          ? { text: caution, refused: false }
          : null;
      stop();
    } catch (err) {
      note = { text: `Could not save: ${err}`, refused: true };
      stop();
    } finally {
      saving = false;
    }
  }

  async function onkeydown(event: KeyboardEvent): Promise<void> {
    if (!recording) return;
    event.preventDefault();
    event.stopPropagation();

    if (event.key === "Escape") {
      stop();
      return;
    }

    if (event.key === "Backspace" && (onreset || onclear)) {
      stop();
      note = null;
      await (onreset ?? onclear)?.();
      return;
    }

    if (event.key === "Delete" && onclear) {
      stop();
      note = null;
      await onclear();
      return;
    }

    const read = chordFor(scope, event);
    if ("held" in read) {
      held = read.held;
      note = null;
      return;
    }
    if ("refused" in read) {
      note = { text: read.refused, refused: true };
      held = [];
      return;
    }
    await commit(read.chord, read.caution);
  }

  function onkeyup(event: KeyboardEvent): void {
    if (!recording) return;
    // A modifier let go leaves the cursor, so the caps say what is still down.
    const stillHeld: string[] = [];
    if (event.ctrlKey) stillHeld.push("Ctrl");
    if (event.altKey) stillHeld.push("Alt");
    if (event.shiftKey) stillHeld.push("Shift");
    if (event.metaKey) stillHeld.push("Win");
    held = stillHeld;
  }

  /** The control speaks for the row, so the row's own copy of this goes quiet. */
  const standing = $derived.by(() => {
    if (note) return note;
    if (contested) {
      return { text: `${contested} takes this key on the same list, so this one never fires.`, refused: true };
    }
    return null;
  });
</script>

<div class="recorder">
  <button
    bind:this={button}
    type="button"
    class="key"
    class:recording
    class:refused={Boolean(contested)}
    class:blank={!chord && !recording}
    aria-label={ariaLabel}
    aria-pressed={recording}
    data-recording={recording || undefined}
    disabled={saving}
    onclick={toggle}
    {onkeydown}
    {onkeyup}
    onblur={stop}
  >
    {#if recording}
      {#if held.length > 0}
        <Chord chord={held.join("+")} dim />
      {/if}
      <kbd class="sill-key cursor" aria-hidden="true">…</kbd>
      <span class="prompt">{held.length > 0 ? "then a key" : "Press a key"}</span>
    {:else if chord}
      <Chord {chord} />
    {:else}
      <span class="prompt">{placeholder}</span>
    {/if}
  </button>

  {#if onclear && chord && !recording}
    <button
      type="button"
      class="clear"
      aria-label="Take this key away"
      onclick={() => {
        note = null;
        void onclear();
      }}
    >
      ×
    </button>
  {/if}
</div>

{#if standing}
  <p class="note" class:refused={standing.refused} use:hint={standing.more}>{standing.text}</p>
{/if}

<style>
  .recorder {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
  }

  .key {
    display: inline-flex;
    align-items: center;
    /* The caps sit in the middle of the control, however wide it is. */
    justify-content: center;
    gap: var(--space-1);
    min-width: 118px;
    height: var(--control-height);
    padding: 0 var(--space-2);
    border: 0;
    border-radius: var(--radius-md);
    background: var(--fill-1);
    box-shadow: var(--ring);
    color: var(--text-1);
    font: inherit;
    font-size: var(--text-meta);
    cursor: pointer;
    transition:
      background-color var(--motion-state) var(--ease),
      box-shadow var(--motion-state) var(--ease);
  }

  .key:hover:not(:disabled) {
    background: var(--fill-2);
  }

  /* Focus and recording are the two things the accent is for here. */
  .key:focus-visible {
    box-shadow: var(--focus-ring);
  }

  .key.recording {
    background: var(--fill-2);
    box-shadow: var(--ring-accent-faint);
  }

  .key.refused {
    box-shadow: var(--ring-danger);
  }

  .key:disabled {
    opacity: var(--opacity-disabled);
    cursor: default;
  }

  .prompt {
    color: var(--text-3);
  }

  .blank .prompt {
    color: var(--text-2);
  }

  /* The cap that is waiting for its key. */
  .cursor {
    color: var(--accent-bright);
    animation: waiting var(--motion-pulse) ease-in-out infinite;
  }

  @keyframes waiting {
    50% {
      opacity: var(--opacity-faint);
    }
  }

  .clear {
    display: grid;
    place-items: center;
    width: var(--icon-tile-sm);
    height: var(--icon-tile-sm);
    border: 0;
    border-radius: var(--radius-pill);
    background: transparent;
    color: var(--text-3);
    font: inherit;
    font-size: var(--text-body);
    line-height: 1;
    cursor: pointer;
    transition:
      background-color var(--motion-state) var(--ease),
      color var(--motion-state) var(--ease);
  }

  .clear:hover {
    background: var(--danger-fill);
    color: var(--danger);
  }

  /* Under the control, in the column the control is in, so the reason a key
     was refused is beside the key and not several screens away. */
  .note {
    margin: var(--space-1) 0 0;
    max-width: 36ch;
    font-size: var(--text-meta);
    line-height: 1.45;
    color: var(--text-2);
    text-align: right;
  }

  .note.refused {
    color: var(--danger);
  }

  @media (prefers-reduced-motion: reduce) {
    .cursor {
      animation: none;
    }
  }
</style>

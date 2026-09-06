<!--
  The keyboard, with every bound key lit.

  ## Why a picture of a keyboard

  A list of chords answers "what does Ctrl+K do" only if you know to look for
  Ctrl+K. A keyboard answers "what is on Ctrl" at a glance, and it answers the
  question a person actually has when choosing a key, which is "what is
  free". Hold a modifier over it and it shows that layer; hover a lit key and
  it says what the key does; click one and the panel takes you to the row
  that set it.

  ## What it costs

  Nothing at rest. It is drawn from the keyboard reference the panel already
  holds, it asks Rust for nothing, and the only listener it has is on its own
  element, so a modifier pressed anywhere else in the window reaches nothing
  here. Which keys are bound, and which of them clash, is Rust's answer; this
  only decides which cap to light.
-->
<script lang="ts">
  import { hint } from "$lib/hint";
  import { keyOf, modifiersOf, SECTIONS } from "$lib/keys";
  import type { KeyLine, KeySection } from "$lib/exthost/commands";

  interface Props {
    sections: KeySection[];
    /** A lit key was chosen: the chord it carries. */
    onpick?: (chord: string) => void;
  }

  let { sections, onpick }: Props = $props();

  /** One cap: what is printed on it, and how wide it is in quarter units. */
  interface Cap {
    label: string;
    w: number;
    /** A modifier, which lights when it is in the layer being shown. */
    modifier?: boolean;
  }

  /*
   * An ANSI board without the function row, plus the arrows, in quarter
   * units so the rows all add up to sixty. Labels are the names `keysOf`
   * draws, so a chord's key finds its cap by the text on it.
   */
  const ROWS: Cap[][] = [
    [
      { label: "`", w: 4 },
      ...["1", "2", "3", "4", "5", "6", "7", "8", "9", "0", "-", "="].map((label) => ({ label, w: 4 })),
      { label: "⌫", w: 8 },
    ],
    [
      { label: "Tab", w: 6 },
      ...["Q", "W", "E", "R", "T", "Y", "U", "I", "O", "P", "[", "]"].map((label) => ({ label, w: 4 })),
      { label: "\\", w: 6 },
    ],
    [
      { label: "Caps", w: 7 },
      ...["A", "S", "D", "F", "G", "H", "J", "K", "L", ";", "'"].map((label) => ({ label, w: 4 })),
      { label: "↵", w: 9 },
    ],
    [
      { label: "Shift", w: 9, modifier: true },
      ...["Z", "X", "C", "V", "B", "N", "M", ",", ".", "/"].map((label) => ({ label, w: 4 })),
      { label: "Shift", w: 11, modifier: true },
    ],
    [
      { label: "Ctrl", w: 5, modifier: true },
      { label: "Win", w: 5, modifier: true },
      { label: "Alt", w: 5, modifier: true },
      { label: "Space", w: 15 },
      { label: "Alt", w: 5, modifier: true },
      { label: "Ctrl", w: 5, modifier: true },
      { label: "←", w: 5 },
      { label: "↑", w: 5 },
      { label: "↓", w: 5 },
      { label: "→", w: 5 },
    ],
  ];

  const MODIFIERS = ["Ctrl", "Alt", "Shift", "Win"];

  type Reach = "both" | "launcher" | "anywhere";
  const REACHES: { id: Reach; label: string }[] = [
    { id: "both", label: "Everything" },
    { id: "anywhere", label: "From anywhere" },
    { id: "launcher", label: "In the launcher" },
  ];

  /** The layer chosen with the chips. */
  let chosen = $state<Set<string>>(new Set());
  /** The layer the keyboard is holding down right now, which wins while it is. */
  let held = $state<Set<string>>(new Set());
  let reach = $state<Reach>("both");

  const layer = $derived(held.size > 0 ? held : chosen);

  function toggleModifier(name: string): void {
    const next = new Set(chosen);
    if (next.has(name)) next.delete(name);
    else next.add(name);
    chosen = next;
  }

  /** The held modifiers, read off the event so a release is seen too. */
  function follow(event: KeyboardEvent): void {
    const next = new Set<string>();
    if (event.ctrlKey) next.add("Ctrl");
    if (event.altKey) next.add("Alt");
    if (event.shiftKey) next.add("Shift");
    if (event.metaKey) next.add("Win");
    held = next;
  }

  function sameLayer(chord: string): boolean {
    const mods = modifiersOf(chord);
    return mods.length === layer.size && mods.every((one) => layer.has(one));
  }

  function inReach(section: string): boolean {
    if (reach === "both") return true;
    const anywhere = section === SECTIONS.opening || section === SECTIONS.anywhere;
    return reach === "anywhere" ? anywhere : !anywhere;
  }

  /** Every line in the layer being shown, by the cap it lands on. */
  const lit = $derived.by(() => {
    const byCap = new Map<string, { line: KeyLine; section: string }[]>();
    for (const section of sections) {
      if (!inReach(section.title)) continue;
      for (const line of section.keys) {
        if (!sameLayer(line.chord)) continue;
        const cap = keyOf(line.chord);
        if (!cap) continue;
        const at = byCap.get(cap) ?? [];
        at.push({ line, section: section.title });
        byCap.set(cap, at);
      }
    }
    return byCap;
  });

  /** Keys that are bound and not drawn on this board, said under it. */
  const offBoard = $derived.by(() => {
    const drawn = new Set(ROWS.flat().map((cap) => cap.label));
    const missing: { line: KeyLine; section: string }[] = [];
    for (const section of sections) {
      if (!inReach(section.title)) continue;
      for (const line of section.keys) {
        if (!sameLayer(line.chord)) continue;
        const cap = keyOf(line.chord);
        if (cap && !drawn.has(cap)) missing.push({ line, section: section.title });
      }
    }
    return missing;
  });

  function saysOf(at: { line: KeyLine; section: string }[] | undefined): string {
    if (!at || at.length === 0) return "";
    return at
      .map(({ line, section }) => {
        const state = line.refused ? ", refused by Windows" : line.contested ? ", contested" : "";
        return `${line.chord} · ${line.does} (${section}${state})`;
      })
      .join("\n");
  }

  function broken(at: { line: KeyLine }[] | undefined): boolean {
    return Boolean(at?.some(({ line }) => line.refused || line.contested));
  }

  const count = $derived([...lit.values()].reduce((all, at) => all + at.length, 0));
</script>

<div class="map">
  <div class="chips" role="group" aria-label="Which keys to show">
    {#each MODIFIERS as name (name)}
      <button
        type="button"
        class="chip"
        class:on={layer.has(name)}
        aria-pressed={layer.has(name)}
        onclick={() => toggleModifier(name)}
      >
        {name}
      </button>
    {/each}
    <span class="gap"></span>
    {#each REACHES as one (one.id)}
      <button
        type="button"
        class="chip quiet"
        class:on={reach === one.id}
        aria-pressed={reach === one.id}
        onclick={() => (reach = one.id)}
      >
        {one.label}
      </button>
    {/each}
  </div>

  <!--
    The board takes focus so a held modifier can show its layer. The listener
    is on this element alone: nothing about the window is watched.
  -->
  <div
    class="board"
    tabindex="0"
    role="toolbar"
    aria-label="The keyboard, with every bound key lit"
    onkeydown={follow}
    onkeyup={follow}
    onblur={() => (held = new Set())}
  >
    {#each ROWS as row, r (r)}
      {#each row as cap, c (`${r}-${c}`)}
        {@const at = cap.modifier ? undefined : lit.get(cap.label)}
        {@const on = cap.modifier ? layer.has(cap.label) : Boolean(at && at.length > 0)}
        <button
          type="button"
          class="cap"
          class:on
          class:modifier={cap.modifier}
          class:broken={broken(at)}
          style:grid-column="span {cap.w}"
          tabindex="-1"
          disabled={!on || cap.modifier}
          use:hint={saysOf(at)}
          onclick={() => at?.[0] && onpick?.(at[0].line.chord)}
        >
          {cap.label}
          {#if at && at.length > 1}<span class="more">{at.length}</span>{/if}
        </button>
      {/each}
    {/each}
  </div>

  <div class="legend">
    <span class="swatch on"></span>
    <span>{count} {count === 1 ? "key" : "keys"} on this layer</span>
    <span class="swatch broken"></span>
    <span>refused or contested</span>
    {#if held.size > 0}
      <span class="holding">showing what is held</span>
    {:else}
      <span class="holding">hold a modifier over the board to see its layer</span>
    {/if}
  </div>

  {#if offBoard.length > 0}
    <p class="off">
      Also on this layer:
      {#each offBoard as one, at (`${one.line.chord}-${at}`)}
        <button type="button" class="off-key" onclick={() => onpick?.(one.line.chord)}>
          {one.line.chord} · {one.line.does}
        </button>
      {/each}
    </p>
  {/if}
</div>

<style>
  .map {
    display: grid;
    gap: var(--space-2);
  }

  .chips {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: var(--space-1);
  }

  .gap {
    flex: 1;
  }

  .chip {
    padding: var(--space-half) var(--space-2);
    border: 0;
    border-radius: var(--radius-pill);
    background: var(--fill-2);
    box-shadow: var(--bevel-tile);
    color: var(--text-2);
    font: inherit;
    font-size: var(--text-meta);
    font-weight: var(--weight-medium);
    cursor: pointer;
    transition:
      background-color var(--motion-state) var(--ease),
      color var(--motion-state) var(--ease);
  }

  .chip:hover {
    color: var(--text-1);
  }

  .chip.quiet {
    background: transparent;
    box-shadow: none;
  }

  /* Chosen is affirmative state, so the accent. */
  .chip.on {
    background: var(--accent-fill-strong);
    color: var(--text-1);
    box-shadow: var(--ring-accent-faint);
  }

  .board {
    display: grid;
    grid-template-columns: repeat(60, minmax(0, 1fr));
    grid-auto-rows: var(--keymap-row);
    gap: var(--space-half);
    padding: var(--space-2);
    border-radius: var(--radius-lg);
    background: var(--fill-1);
    box-shadow: var(--well);
    outline: none;
  }

  .board:focus-visible {
    box-shadow: var(--well), var(--focus-ring);
  }

  .cap {
    display: grid;
    place-items: center;
    min-width: 0;
    padding: 0;
    border: 0;
    border-radius: var(--radius-sm);
    background: var(--fill-2);
    box-shadow: var(--bevel-tile);
    color: var(--text-3);
    font: inherit;
    font-size: var(--text-label);
    font-weight: var(--weight-medium);
    overflow: hidden;
    white-space: nowrap;
    cursor: default;
    transition:
      background-color var(--motion-state) var(--ease),
      color var(--motion-state) var(--ease),
      box-shadow var(--motion-state) var(--ease);
  }

  .cap.modifier {
    color: var(--text-3);
  }

  /* Bound is affirmative state: the accent, and a real cursor. */
  .cap.on {
    background: var(--accent-fill-strong);
    box-shadow: var(--ring-accent);
    color: var(--text-1);
    cursor: pointer;
  }

  .cap.on:hover:not(:disabled) {
    background: var(--accent-fill-strong);
    box-shadow: var(--ring-accent), var(--elevation-thumb);
  }

  .cap.on.modifier {
    cursor: default;
  }

  .cap.broken {
    background: var(--danger-fill);
    box-shadow: var(--ring-danger);
  }

  .more {
    position: absolute;
    right: var(--space-half);
    top: var(--space-half);
    font-size: var(--text-micro);
    color: var(--text-2);
  }

  .cap {
    position: relative;
  }

  .legend {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: var(--space-2);
    font-size: var(--text-meta);
    color: var(--text-3);
  }

  .swatch {
    width: var(--space-3);
    height: var(--space-3);
    border-radius: var(--radius-xs);
    background: var(--accent-fill-strong);
    box-shadow: var(--ring-accent);
  }

  .swatch.broken {
    background: var(--danger-fill);
    box-shadow: var(--ring-danger);
  }

  .holding {
    margin-left: auto;
    color: var(--text-3);
  }

  .off {
    margin: 0;
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-1) var(--space-2);
    font-size: var(--text-meta);
    color: var(--text-3);
  }

  .off-key {
    padding: var(--space-half) var(--space-2);
    border: 0;
    border-radius: var(--radius-pill);
    background: var(--fill-2);
    color: var(--text-2);
    font: inherit;
    font-size: var(--text-meta);
    cursor: pointer;
  }

  .off-key:hover {
    color: var(--text-1);
    background: var(--fill-3);
  }
</style>

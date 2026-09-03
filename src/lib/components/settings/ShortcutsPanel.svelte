<script lang="ts">
  /**
   * Keys that run an action without the launcher appearing.
   *
   * The list of actions is asked for rather than written here: it is the same
   * registry Enter and the action panel use, so a transform added in Rust
   * becomes bindable without this file changing.
   */
  import { onMount } from "svelte";
  import Section from "./Section.svelte";
  import Row from "./Row.svelte";
  import Button from "./Button.svelte";
  import Toggle from "../Toggle.svelte";
  import Segmented from "./Segmented.svelte";
  import Select from "./Select.svelte";
  import Instead from "../Instead.svelte";
  import { standing } from "$lib/instead";
  import { actionsFor, searchCommands, type ActionInfo } from "$lib/exthost/commands";
  import {
    actionShortcuts,
    chordFrom,
    navigationKeys,
    type ActionShortcut,
    type NavigationKey,
  } from "$lib/settings";
  import type { Binding, BindingSource, Preferences, TapModifier } from "$lib/settings";

  interface Props {
    prefs: Preferences;
    commit: (next: Preferences) => void;
    /**
     * Accelerators Windows refused, from the same list the hotkey rows use.
     *
     * A shortcut another application already owns registers as an error and
     * then looks exactly like one that works: the row shows the key, the key
     * does nothing. These rows had no way to say so at all.
     */
    conflicts: string[];
  }

  let { prefs, commit, conflicts }: Props = $props();

  /**
   * The keys worth offering as a hyper key.
   *
   * A short list rather than a capture field, and deliberately. A hyper key
   * stops doing what is printed on it, so the ones worth offering are the ones
   * almost nobody uses for anything: a free capture invites somebody to bind
   * `A` and then wonder why they cannot type.
   */
  const HYPER_KEYS = [
    { value: "0", label: "Off" },
    { value: "20", label: "Caps Lock" },
    { value: "145", label: "Scroll Lock" },
    { value: "165", label: "Right Alt" },
    { value: "163", label: "Right Ctrl" },
  ];

  const bindings = $derived(prefs.bindings ?? []);

  /** Everything that can be done to text, which is what a key can bind to. */
  let textActions = $state<ActionInfo[]>([]);
  /** Titles for the commands bindings point at, so the list reads as names. */
  let commandNames = $state<Record<string, string>>({});

  let recording = $state<number | null>(null);
  let status = $state("");

  /**
   * How the launcher is moved around.
   *
   * Here rather than in its own panel because this is where somebody looks
   * for "what keys does Sill use", and the two answers being in two places
   * is the fragmentation the index list was built to undo.
   */
  /**
   * The modifiers a double-tap can be bound to, and off.
   *
   * Off first, because it is the default and because a gesture that installs
   * a keyboard hook should be something somebody chooses rather than
   * something they have to find and turn off.
   */
  const TAP_MODIFIERS = [
    { value: "off", label: "Off" },
    { value: "control", label: "Ctrl" },
    { value: "alt", label: "Alt" },
    { value: "shift", label: "Shift" },
    { value: "win", label: "Win" },
  ];

  function setTap(next: string) {
    commit({
      ...prefs,
      taps: {
        ...prefs.taps,
        modifier: next === "off" ? null : (next as TapModifier),
      },
    });
  }

  const PRESETS = [
    { value: "standard", label: "Arrows only" },
    { value: "vim", label: "Vim" },
    { value: "emacs", label: "Emacs" },
  ];

  /**
   * What each movement resolves to.
   *
   * Asked for rather than derived: the answer depends on the preset and on
   * what has been overridden, and a preset can take a key another movement
   * preferred. Working it out again here is how a settings screen ends up
   * naming a key that does something else.
   */
  let moves = $state<NavigationKey[]>([]);
  let rebinding = $state<string | null>(null);

  async function loadMoves() {
    moves = await navigationKeys();
  }

  onMount(() => void loadMoves());

  async function setPreset(next: string) {
    commit({
      ...prefs,
      navigation: { ...prefs.navigation, preset: next as "standard" | "vim" | "emacs" },
    });
    // The commit is asynchronous and the resolved map lives in Rust, so the
    // rows are re-read rather than guessed at.
    setTimeout(() => void loadMoves(), 120);
  }

  function rebind(event: KeyboardEvent, move: NavigationKey) {
    event.preventDefault();
    event.stopPropagation();

    if (event.key === "Escape") {
      rebinding = null;
      return;
    }

    // Backspace gives the movement back to the preset.
    if (event.key === "Backspace") {
      const overrides = { ...prefs.navigation.overrides };
      delete overrides[move.id];
      commit({ ...prefs, navigation: { ...prefs.navigation, overrides } });
      rebinding = null;
      setTimeout(() => void loadMoves(), 120);
      return;
    }

    const chord = chordFrom(event);
    if (!chord) return;

    commit({
      ...prefs,
      navigation: {
        ...prefs.navigation,
        overrides: { ...prefs.navigation.overrides, [move.id]: chord },
      },
    });
    rebinding = null;
    setTimeout(() => void loadMoves(), 120);
  }

  onMount(async () => {
    textActions = await actionsFor("text");
  });

  /**
   * The key that runs each action, and what is contesting it.
   *
   * Asked for rather than worked out here for the same reason the movements
   * are: an action ships with a chord, a person may have replaced it, and
   * whether two of them clash depends on which lists they appear on together.
   * All three answers live in Rust, and computing any of them again here is
   * how a settings screen ends up naming a key that does something else.
   */
  let actionKeys = $state<ActionShortcut[]>([]);
  let rebindingAction = $state<string | null>(null);
  let actionStatus = $state("");

  async function loadActionKeys() {
    actionKeys = await actionShortcuts();
  }

  onMount(() => void loadActionKeys());

  /** Saves one action's key, or clears it, and re-reads what that resolved to. */
  function setActionKey(id: string, chord: string | null) {
    const overrides = { ...(prefs.actionKeys?.overrides ?? {}) };

    if (chord === null) delete overrides[id];
    else overrides[id] = chord;

    commit({ ...prefs, actionKeys: { overrides } });
    rebindingAction = null;
    // The commit is asynchronous and the resolved chord, along with any
    // conflict it just created, is worked out in Rust. The rows are re-read
    // rather than guessed at.
    setTimeout(() => void loadActionKeys(), 120);
  }

  function rebindAction(event: KeyboardEvent, row: ActionShortcut) {
    event.preventDefault();
    event.stopPropagation();

    if (event.key === "Escape") {
      rebindingAction = null;
      return;
    }

    // Backspace gives the action back the key it shipped with; Delete takes
    // the key away entirely. Two different things, and a list where some rows
    // ship with a key and some do not needs both.
    if (event.key === "Backspace") {
      actionStatus = "";
      setActionKey(row.id, null);
      return;
    }

    if (event.key === "Delete") {
      actionStatus = "";
      setActionKey(row.id, "");
      return;
    }

    // The Windows key reaches the launcher as the same held key Ctrl does, so
    // a chord saved with it would fire on the Ctrl version of itself. Rust
    // refuses it as well; saying so here saves pressing it twice.
    if (event.metaKey) {
      actionStatus = "The Windows key cannot run an action";
      return;
    }

    // A bare key is the letter itself. The launcher only reads an action chord
    // when nothing is being typed, but its search field has focus the whole
    // time, so binding `c` would be binding a character somebody meant to type.
    if (!event.ctrlKey && !event.altKey) {
      actionStatus = "An action key needs at least one of Ctrl or Alt";
      return;
    }

    const chord = chordFrom(event);
    if (!chord) return;

    actionStatus = "";
    setActionKey(row.id, chord);
  }

  $effect(() => {
    // Resolve any command a binding names, so the row can show "Discord"
    // rather than "app:C:\...\Discord.lnk".
    for (const binding of bindings) {
      if (binding.source.from !== "command") continue;
      const id = binding.source.id;
      if (id in commandNames) continue;

      commandNames = { ...commandNames, [id]: id };
      void searchCommands("").then((all) => {
        const found = all.find((c) => c.id === id);
        if (found) commandNames = { ...commandNames, [id]: found.title };
      });
    }
  });

  function save(next: Binding[]) {
    commit({ ...prefs, bindings: next });
  }

  function add() {
    save([
      ...bindings,
      {
        accelerator: "",
        action: textActions[0]?.id ?? "sill.text.upper",
        source: { from: "selection" },
        replace: true,
      },
    ]);
    recording = bindings.length;
  }

  function update(at: number, patch: Partial<Binding>) {
    save(bindings.map((b, i) => (i === at ? { ...b, ...patch } : b)));
  }

  function remove(at: number) {
    save(bindings.filter((_, i) => i !== at));
  }

  /**
   * Turns a key press into an accelerator string.
   *
   * A modifier on its own is not a shortcut, and binding one would swallow
   * every Ctrl press on the machine.
   */
  function record(event: KeyboardEvent, at: number) {
    event.preventDefault();
    event.stopPropagation();

    if (event.key === "Escape") {
      recording = null;
      return;
    }

    const key = event.key;
    if (["Control", "Alt", "Shift", "Meta", "OS"].includes(key)) return;

    const parts: string[] = [];
    if (event.ctrlKey) parts.push("Ctrl");
    if (event.altKey) parts.push("Alt");
    if (event.shiftKey) parts.push("Shift");
    if (event.metaKey) parts.push("Super");

    // A bare letter would take that key away from every application on the
    // machine, which is not a thing anybody means to do.
    if (parts.length === 0) {
      status = "A shortcut needs at least one of Ctrl, Alt or Shift";
      return;
    }

    const named = key.length === 1 ? key.toUpperCase() : key;
    const accelerator = [...parts, named].join("+");

    if (bindings.some((b, i) => i !== at && b.accelerator === accelerator)) {
      status = `${accelerator} is already used`;
      return;
    }

    status = "";
    update(at, { accelerator });
    recording = null;
  }

  function describe(source: BindingSource): string {
    if (source.from === "selection") return "the selected text";
    if (source.from === "clipboard") return "the clipboard";
    return commandNames[source.id] ?? source.id;
  }
</script>

<Section
  label="Shortcuts"
  description="A key that runs an action on whatever is selected, without the launcher appearing. Highlight some text, press the key, and the text changes where it sits."
>
  {#each bindings as binding, at (at)}
    <Row
      title={textActions.find((a) => a.id === binding.action)?.title ?? binding.action}
      description={binding.accelerator && conflicts.includes(binding.accelerator)
        ? "Another application already has this combination, so it does nothing. Choose a different one."
        : `Runs on ${describe(binding.source)}`}
    >
      <div class="controls">
        <button
          class="key"
          class:taken={!!binding.accelerator && conflicts.includes(binding.accelerator)}
          class:recording={recording === at}
          onclick={() => (recording = recording === at ? null : at)}
          onkeydown={(e) => recording === at && record(e, at)}
        >
          {#if recording === at}
            Press a key…
          {:else if binding.accelerator}
            {binding.accelerator}
          {:else}
            Set a key
          {/if}
        </button>

        <Select
          value={binding.action}
          options={textActions.map((action) => ({ value: action.id, label: action.title }))}
          onchange={(next) => update(at, { action: next })}
          ariaLabel="What it does"
        />

        <Select
          value={binding.source.from}
          options={[
            { value: "selection", label: "Selection" },
            { value: "clipboard", label: "Clipboard" },
          ]}
          onchange={(next) =>
            update(at, {
              source: next === "clipboard" ? { from: "clipboard" } : { from: "selection" },
            })}
          ariaLabel="What it runs on"
        />

        <Button label="Remove" tone="danger" onclick={() => remove(at)} />
      </div>
    </Row>
  {/each}

  <Instead
    tone={standing({ failed: false, loading: false, count: bindings.length })}
    inline
    headline="No shortcuts yet"
    hint="One that upper-cases the selection is a good first one."
  />

  <Row
    title="Put the result back"
    description="Replaces the selected text with what the action produced. Off means the result is only copied."
  >
    <Toggle
      checked={bindings.length > 0 && bindings.every((b) => b.replace)}
      onchange={(on: boolean) => save(bindings.map((b) => ({ ...b, replace: on })))}
    />
  </Row>

  <div class="foot">
    <Button label="Add a shortcut" onclick={add} />
    {#if status}<span class="status">{status}</span>{/if}
  </div>
</Section>

<Section
  label="Double-tap"
  description="Tapping a modifier twice opens the launcher. It needs no chord and no key anything else wants: the modifier keeps doing its own job, and doing it twice quickly is a thing nothing else listens for."
>
  <Row
    title="Open with a double-tap"
    description="Anything typed between the two taps cancels it, so an ordinary shortcut never sets it off. Watching for this installs a keyboard hook, which is why it is off until you ask for it."
  >
    {#snippet control()}
      <Segmented
        label="Open with a double-tap"
        value={prefs.taps?.modifier ?? "off"}
        options={TAP_MODIFIERS}
        onchange={setTap}
      />
    {/snippet}
  </Row>
</Section>

<Section
  label="Moving around"
  description="A preset adds keys, it never takes the arrows away. Where a preset wants a key something else was using, the displaced one falls back to its second choice and the row below shows what actually happens."
>
  <Row
    title="Extra keys"
    description="Ctrl rather than bare letters throughout, and that is forced rather than chosen: the search field has focus the whole time, so a bare j is the letter j."
  >
    {#snippet control()}
      <Segmented
        label="Extra keys"
        value={prefs.navigation.preset}
        options={PRESETS}
        onchange={(next) => void setPreset(next)}
      />
    {/snippet}
  </Row>

  <Row
    title="Jump to a row by number"
    description="Ctrl and a digit opens that result. Ctrl for the same reason as the presets: a bare 3 is the character three."
  >
    {#snippet control()}
      <Toggle
        checked={prefs.navigation.numeric}
        onchange={(on: boolean) =>
          commit({ ...prefs, navigation: { ...prefs.navigation, numeric: on } })}
      />
    {/snippet}
  </Row>

  {#each moves as move (move.id)}
    <Row title={move.title} description={move.overridden ? "Set by hand" : ""}>
      {#snippet control()}
        <button
          class="key"
          class:recording={rebinding === move.id}
          onclick={() => (rebinding = rebinding === move.id ? null : move.id)}
          onkeydown={(e) => rebinding === move.id && rebind(e, move)}
        >
          {#if rebinding === move.id}
            Press a key…
          {:else}
            {move.chord.split("+").join(" ")}
          {/if}
        </button>
      {/snippet}
    </Row>
  {/each}
</Section>

<Section
  label="Action keys"
  description="A key that runs one of the actions on the selected result without opening the action panel first. The panel draws these down its right hand side, so the key is on screen beside the thing it does. Backspace puts one back to what it shipped with; Delete takes the key away."
>
  {#each actionKeys as row (row.id)}
    <Row
      title={row.title}
      description={row.contested
        ? `${row.chord} already runs ${row.contested} on the same list, so this one never fires. Choose a different key.`
        : row.overridden
          ? "Set by hand"
          : ""}
    >
      {#snippet control()}
        <button
          class="key"
          class:taken={!!row.contested}
          class:recording={rebindingAction === row.id}
          onclick={() => (rebindingAction = rebindingAction === row.id ? null : row.id)}
          onkeydown={(e) => rebindingAction === row.id && rebindAction(e, row)}
        >
          {#if rebindingAction === row.id}
            Press a key…
          {:else if row.chord}
            {row.chord.split("+").join(" ")}
          {:else}
            No key
          {/if}
        </button>
      {/snippet}
    </Row>
  {/each}

  {#if actionStatus}<span class="status">{actionStatus}</span>{/if}
</Section>

<Section
  label="Hyper key"
  description="One key that stands in for Ctrl, Alt, Shift and Windows together, so every letter becomes a shortcut nothing else has claimed. The key stops doing what is printed on it while this is on."
>
  <Row
    title="Key"
    description="Off by default. Each keystroke sends the whole chord and releases it in the same breath, so nothing can be left held down if Sill stops."
  >
    <Select
      value={String(prefs.hyper?.key ?? 0)}
      options={HYPER_KEYS}
      onchange={(value) => {
        const key = Number(value);
        commit({ ...prefs, hyper: { key: key === 0 ? null : key } });
      }}
    />
  </Row>
</Section>

<style>
  .controls {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    flex-wrap: wrap;
  }

  .key {
    min-width: 118px;
    padding: var(--space-1) var(--space-2);
    font: inherit;
    font-size: var(--text-meta);
    font-variant-numeric: tabular-nums;
    color: var(--text-1);
    background: var(--fill-1);
    border: 1px solid var(--hairline);
    border-radius: var(--radius-md);
    cursor: pointer;
  }

  /* The same red the hotkey rows use for a key another application owns. */
  .key.taken {
    color: var(--danger);
    border-color: var(--danger);
  }

  .key.recording {
    color: var(--accent-bright);
    border-color: var(--accent-bright);
  }

  .foot {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding-top: var(--space-2);
  }

  .status {
    font-size: var(--text-meta);
    color: var(--text-2);
  }
</style>

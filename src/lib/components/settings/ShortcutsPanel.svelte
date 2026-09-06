<script lang="ts">
  /**
   * Every key Sill answers to, in one place, with the keyboard at the top.
   *
   * ## What this panel decides, and what it does not
   *
   * Nothing about which keys are taken. That is `key_owners` in Rust, asked
   * by each recorder before it saves, and it reads the same sheet the keyboard
   * reference draws. Nothing about which movement a chord resolves to, which
   * action a chord runs, or which of two actions wins a chord: `navigation_keys`
   * and `action_shortcuts` answer those, and the rows are re-read after every
   * write rather than guessed at. This file holds focus, recording state, the
   * filter typed into the action list, and the shape of the page.
   *
   * ## Why the global keys moved in here
   *
   * They were drawn by the settings page itself, two hundred lines above this
   * component, with a recorder of their own that listened on the whole window
   * and never disarmed. Two recorders with two looks in one scrolling panel
   * was the visible half of that; the other half was that arming the summon
   * recorder and then pressing a key over any other row rebound the summon key.
   * One recorder, one look, one listener per control.
   *
   * ## What it costs
   *
   * Three reads when the panel opens and again after each write, all awaited.
   * The four timers that used to re-read the rows a hundred and twenty
   * milliseconds after a write are gone: the write is awaited instead.
   */
  import { onMount, tick } from "svelte";
  import Section from "./Section.svelte";
  import Row from "./Row.svelte";
  import Button from "./Button.svelte";
  import Toggle from "../Toggle.svelte";
  import Segmented from "./Segmented.svelte";
  import Select from "./Select.svelte";
  import TextField from "./TextField.svelte";
  import Instead from "../Instead.svelte";
  import KeyRecorder from "./KeyRecorder.svelte";
  import KeyMap from "./KeyMap.svelte";
  import { standing } from "$lib/instead";
  import { SECTIONS } from "$lib/keys";
  import {
    actionsFor,
    keyboardReference,
    searchCommands,
    type ActionInfo,
    type KeySection,
  } from "$lib/exthost/commands";
  import {
    actionShortcuts,
    navigationKeys,
    type ActionShortcut,
    type NavigationKey,
  } from "$lib/settings";
  import type { Binding, BindingSource, Layout, Preferences, TapModifier } from "$lib/settings";

  interface Props {
    prefs: Preferences;
    /** Saves a whole settings object. Awaited, so the rows can be re-read. */
    commit: (next: Preferences) => Promise<void>;
    /**
     * Accelerators Windows refused.
     *
     * A shortcut another application already owns registers as an error and
     * then looks exactly like one that works: the row shows the key, the key
     * does nothing. The recorder says so, in the row that set it.
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

  const PRESETS = [
    { value: "standard", label: "Arrows only" },
    { value: "vim", label: "Vim" },
    { value: "emacs", label: "Emacs" },
  ];

  /** The other keys that work whatever is in front. The summon key is drawn on its own. */
  type GlobalKey = "switcher" | "capture" | "captureScreen";
  const GLOBAL: { id: GlobalKey; title: string; description: string }[] = [
    {
      id: "switcher",
      title: "Window switcher hotkey",
      description: "Opens Sill straight onto the windows you have open, most recent first.",
    },
    {
      id: "capture",
      title: "Screenshot hotkey",
      description: "Picks an area of the screen without opening Sill first. Drag an area, or click a window.",
    },
    {
      id: "captureScreen",
      title: "Whole screen hotkey",
      description: "Copies everything on every display at once, with nothing to pick.",
    },
  ];

  /** What Rust says the keys are. Re-read after every write. */
  let reference = $state<KeySection[]>([]);
  let moves = $state<NavigationKey[]>([]);
  let actionKeys = $state<ActionShortcut[]>([]);

  /** Everything that can be done to text, a window and a folder: what a binding can run. */
  let textActions = $state<ActionInfo[]>([]);
  let windowActions = $state<ActionInfo[]>([]);
  let folderActions = $state<ActionInfo[]>([]);
  /** Titles for the commands bindings point at, so the list reads as names. */
  let commandNames = $state<Record<string, string>>({});

  async function refresh(): Promise<void> {
    const [sheet, moving, acting] = await Promise.all([
      keyboardReference().catch(() => [] as KeySection[]),
      navigationKeys(),
      actionShortcuts(),
    ]);
    reference = sheet;
    moves = moving;
    actionKeys = acting;
  }

  onMount(() => {
    void refresh();
    void Promise.all([actionsFor("text"), actionsFor("window"), actionsFor("folder")]).then(
      ([text, window, folder]) => {
        textActions = text;
        windowActions = window;
        folderActions = folder;
      },
    );
  });

  /** Writes, then reads back what the write resolved to. No timer, no guess. */
  async function save(next: Preferences): Promise<void> {
    await commit(next);
    await refresh();
  }

  const bindings = $derived(prefs.bindings ?? []);
  const layouts = $derived(prefs.layouts ?? []);

  function setHotkey(id: "summon" | GlobalKey, chord: string): Promise<void> {
    return save({ ...prefs, hotkey: { ...prefs.hotkey, [id]: chord } });
  }

  function setTap(next: string): void {
    void save({
      ...prefs,
      taps: { ...prefs.taps, modifier: next === "off" ? null : (next as TapModifier) },
    });
  }

  function setPreset(next: string): void {
    void save({
      ...prefs,
      navigation: { ...prefs.navigation, preset: next as "standard" | "vim" | "emacs" },
    });
  }

  /** A movement's key, or `null` to give it back to the preset. */
  function setMove(id: NavigationKey["id"], chord: string | null): Promise<void> {
    const overrides = { ...prefs.navigation.overrides };
    if (chord === null) delete overrides[id];
    else overrides[id] = chord;
    return save({ ...prefs, navigation: { ...prefs.navigation, overrides } });
  }

  /** An action's key: a chord, `""` for no key at all, or `null` for what it shipped with. */
  function setActionKey(id: string, chord: string | null): Promise<void> {
    const overrides = { ...(prefs.actionKeys?.overrides ?? {}) };
    if (chord === null) delete overrides[id];
    else overrides[id] = chord;
    return save({ ...prefs, actionKeys: { overrides } });
  }

  /**
   * Opening the action panel, offered as though it were an action.
   *
   * It is not one, and deliberately: `bindings::PANEL` is read before the
   * registry is asked anything, because an action called "Show Actions" would
   * appear in every action panel in the launcher including the one it opens.
   * The settings row still has to offer it, so the one place it is spelled out
   * is here, next to the id Rust reads.
   */
  const PANEL: ActionInfo = { id: "sill.actions", title: "Open the Action Panel", primary: false };

  /** The actions a binding may choose from, which follows what it runs on. */
  function choicesFor(source: BindingSource): ActionInfo[] {
    if (source.from === "foregroundWindow") return windowActions;
    if (source.from === "explorerFolder") return [PANEL, ...folderActions];
    if (source.from === "currentSelection") return [PANEL, ...textActions];
    return textActions;
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

  function saveBindings(next: Binding[]): Promise<void> {
    return save({ ...prefs, bindings: next });
  }

  /** Where a binding row is, for the key that was just added to it. */
  let reveal = $state<number | null>(null);

  async function add(): Promise<void> {
    await saveBindings([
      ...bindings,
      {
        accelerator: "",
        action: textActions[0]?.id ?? "sill.text.upper",
        source: { from: "selection" },
        replace: true,
      },
    ]);
    reveal = bindings.length - 1;
    await showRow(reveal);
  }

  function update(at: number, patch: Partial<Binding>): Promise<void> {
    return saveBindings(bindings.map((b, i) => (i === at ? { ...b, ...patch } : b)));
  }

  function remove(at: number): Promise<void> {
    return saveBindings(bindings.filter((_, i) => i !== at));
  }

  function describe(source: BindingSource): string {
    if (source.from === "selection") return "the selected text";
    if (source.from === "clipboard") return "the clipboard";
    if (source.from === "clipboardImage") return "the last picture copied";
    if (source.from === "foregroundWindow") return "the window in front";
    if (source.from === "currentSelection") return "whatever is selected";
    if (source.from === "explorerFolder") return "the folder open in Explorer";
    return commandNames[source.id] ?? source.id;
  }

  /**
   * Changes what a row runs on, and the action with it.
   *
   * Both together, because an action that cannot be done to the new thing is
   * a key that does nothing, and a row that quietly keeps one is a row that
   * lies. Moving to a different kind takes the first action of that kind,
   * which is the one a person is most likely to have meant.
   */
  function runOn(at: number, from: BindingSource["from"]): void {
    const source = (from === "command" ? { from: "selection" } : { from }) as BindingSource;
    const current = bindings[at];
    const choices = choicesFor(source);
    const keeps = choices.some((action) => action.id === current.action);

    void update(at, { source, action: keeps ? current.action : (choices[0]?.id ?? current.action) });
  }

  /** A stable key for a binding row, so removing one does not re-key the rest. */
  function keyOf(binding: Binding, at: number): string {
    return `${at}:${binding.action}:${JSON.stringify(binding.source)}:${binding.argument ?? ""}`;
  }

  /*
   * Window layouts of your own, kept in preferences and applied by Rust.
   *
   * The fields are fractions of the work area and the panel only holds them:
   * clamping, tiling and the move itself belong to the layout action, so a
   * layout typed here and one named by the model are applied the same way.
   */
  const FRACTIONS = ["x", "y", "width", "height"] as const;

  function saveLayouts(next: Layout[]): Promise<void> {
    return save({ ...prefs, layouts: next });
  }

  function addLayout(): void {
    void saveLayouts([
      ...layouts,
      { id: crypto.randomUUID(), name: `Layout ${layouts.length + 1}`, x: 0, y: 0, width: 0.5, height: 1 },
    ]);
  }

  function updateLayout(at: number, patch: Partial<Layout>): void {
    void saveLayouts(layouts.map((layout, i) => (i === at ? { ...layout, ...patch } : layout)));
  }

  function removeLayout(at: number): void {
    void saveLayouts(layouts.filter((_, i) => i !== at));
  }

  /**
   * A key for one layout: a binding on the window in front, carrying the
   * layout's name as the answer the action would otherwise ask for. The new
   * row is scrolled to, because it lands in a different section.
   */
  async function bindLayout(layout: Layout): Promise<void> {
    await saveBindings([
      ...bindings,
      {
        accelerator: "",
        action: "sill.window.layout",
        source: { from: "foregroundWindow" },
        replace: false,
        argument: layout.name,
      },
    ]);
    reveal = bindings.length - 1;
    await showRow(reveal);
  }

  /** A typed fraction, or nothing while it is half typed. */
  function fraction(text: string): number | null {
    const value = Number(text.trim());
    return text.trim() === "" || Number.isNaN(value) ? null : value;
  }

  /* The action keys: filtered and grouped for reading. Rust chose the groups. */
  let filter = $state("");
  let showUnbound = $state(false);

  const bound = $derived(actionKeys.filter((row) => row.chord).length);

  const shownActions = $derived.by(() => {
    const needle = filter.trim().toLowerCase();
    return actionKeys.filter((row) => {
      if (!showUnbound && !row.chord && !row.contested) return false;
      if (!needle) return true;
      return row.title.toLowerCase().includes(needle) || row.group.toLowerCase().includes(needle);
    });
  });

  /* The map picked a key: take the person to the row that set it. */
  let panel = $state<HTMLDivElement | null>(null);

  async function showRow(at: number): Promise<void> {
    await tick();
    const row = panel?.querySelector<HTMLElement>(`[data-binding="${at}"]`);
    row?.scrollIntoView({ block: "center", behavior: "smooth" });
    row?.querySelector<HTMLButtonElement>("button")?.focus();
  }

  async function showChord(chord: string): Promise<void> {
    await tick();
    const rows = panel?.querySelectorAll<HTMLElement>("[data-chord]") ?? [];
    const row = Array.from(rows).find((one) => one.dataset.chord === chord);
    row?.scrollIntoView({ block: "center", behavior: "smooth" });
    row?.querySelector<HTMLButtonElement>("button")?.focus();
  }
</script>

<div class="shortcuts" bind:this={panel}>
  <Section
    label="Keyboard"
    description="Every key Sill answers to, lit on the keyboard. Choose a modifier above the board, or hold one over it, to see that layer; hover a lit key to read what it does; click one to go to the row that set it."
    bare
  >
    <KeyMap sections={reference} onpick={(chord) => void showChord(chord)} />
  </Section>

  <Section
    label="Opening Sill"
    description="The key that summons the launcher, and the two gestures that can stand in for it."
  >
    <Row
      title="Summon hotkey"
      description="Whatever application is in front. Escape while recording keeps the current key."
    >
      {#snippet control()}
        <span data-chord={prefs.hotkey.summon}>
          <KeyRecorder
            chord={prefs.hotkey.summon}
            scope="hotkey"
            section={SECTIONS.opening}
            taken={conflicts.includes(prefs.hotkey.summon)}
            onsave={(chord) => setHotkey("summon", chord)}
            ariaLabel="Summon hotkey"
          />
        </span>
      {/snippet}
    </Row>

    <Row
      title="Open with a double-tap"
      description="Tapping a modifier twice opens the launcher. It needs no chord and no key anything else wants. Anything typed between the two taps cancels it, so an ordinary shortcut never sets it off."
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

    <Row
      title="Hyper key"
      description="One key that stands in for Ctrl, Alt, Shift and Windows together, so every letter becomes a shortcut nothing else has claimed. The key stops doing what is printed on it while this is on."
    >
      {#snippet control()}
        <Select
          value={String(prefs.hyper?.key ?? 0)}
          options={HYPER_KEYS}
          onchange={(value) => {
            const key = Number(value);
            void save({ ...prefs, hyper: { key: key === 0 ? null : key } });
          }}
          ariaLabel="Hyper key"
        />
      {/snippet}
    </Row>

  </Section>

  <Section
    label="From anywhere"
    description="Keys that work whatever application is in front. The first three open one of Sill's own surfaces; the rest run an action on something without the launcher appearing: highlight some text, press the key, and the text changes where it sits."
  >
    <!-- indexed as "Window switcher hotkey", "Screenshot hotkey", "Whole screen hotkey" -->
    {#each GLOBAL as key (key.id)}
      <Row title={key.title} description={key.description}>
        {#snippet control()}
          <span data-chord={prefs.hotkey[key.id]}>
            <KeyRecorder
              chord={prefs.hotkey[key.id]}
              scope="hotkey"
              section={SECTIONS.anywhere}
              taken={Boolean(prefs.hotkey[key.id]) && conflicts.includes(prefs.hotkey[key.id])}
              onsave={(chord) => setHotkey(key.id, chord)}
              onclear={() => setHotkey(key.id, "")}
              placeholder="Off"
              ariaLabel="{key.title} hotkey"
            />
          </span>
        {/snippet}
      </Row>
    {/each}

    {#each bindings as binding, at (keyOf(binding, at))}
      <div data-binding={at} class:revealed={reveal === at}>
        <Row
          title={choicesFor(binding.source).find((a) => a.id === binding.action)?.title ??
            binding.action}
          description={`Runs on ${describe(binding.source)}`}
        >
          <div class="controls">
            <span data-chord={binding.accelerator}>
              <KeyRecorder
                chord={binding.accelerator}
                scope="binding"
                section={SECTIONS.anywhere}
                taken={Boolean(binding.accelerator) && conflicts.includes(binding.accelerator)}
                onsave={(chord) => update(at, { accelerator: chord })}
                onclear={() => update(at, { accelerator: "" })}
                ariaLabel="Key for this shortcut"
              />
            </span>

            <Select
              value={binding.action}
              options={choicesFor(binding.source).map((action) => ({
                value: action.id,
                label: action.title,
              }))}
              onchange={(next) => void update(at, { action: next })}
              ariaLabel="What it does"
            />

            <Select
              value={binding.source.from}
              options={[
                { value: "currentSelection", label: "Whatever is selected" },
                { value: "selection", label: "Selected text" },
                { value: "clipboard", label: "Clipboard" },
                { value: "clipboardImage", label: "The last picture copied" },
                { value: "foregroundWindow", label: "Window in front" },
                { value: "explorerFolder", label: "Explorer's folder" },
              ]}
              onchange={(next) => runOn(at, next as BindingSource["from"])}
              ariaLabel="What it runs on"
            />

            <Button label="Remove" tone="danger" onclick={() => void remove(at)} />
          </div>
        </Row>
      </div>
    {/each}

    <Instead
      tone={standing({ failed: false, loading: false, count: bindings.length })}
      inline
      headline="No shortcuts of your own yet"
      hint="One that upper-cases the selection is a good first one."
    />


    <!-- not a setting: the one thing this list can be told to do -->
    <Row
      title="Add a shortcut"
      description="A key that runs an action on the selection, the window in front, or a folder, without the launcher appearing."
    >
      {#snippet control()}
        <Button label="Add" onclick={() => void add()} />
      {/snippet}
    </Row>

    {#if bindings.length > 0}
      <Row
        title="Put the result back"
        description="Replaces the selected text with what the action produced. Off means the result is only copied."
      >
        {#snippet control()}
          <Toggle
            checked={bindings.every((b) => b.replace)}
            onchange={(on: boolean) => void saveBindings(bindings.map((b) => ({ ...b, replace: on })))}
          />
        {/snippet}
      </Row>
    {/if}
  </Section>

  <Section
    label="Moving around"
    description="A preset adds keys, it never takes the arrows away. Where a preset wants a key something else was using, the displaced one falls back to its second choice and the row shows what actually happens. Backspace while recording gives a movement back to the preset."
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
          onchange={setPreset}
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
            void save({ ...prefs, navigation: { ...prefs.navigation, numeric: on } })}
        />
      {/snippet}
    </Row>

    {#each moves as move (move.id)}
      <Row title={move.title} description={move.overridden ? "Set by hand" : ""}>
        {#snippet control()}
          <span data-chord={move.chord}>
            <KeyRecorder
              chord={move.chord}
              scope="navigation"
              section={SECTIONS.moving}
              onsave={(chord) => setMove(move.id, chord)}
              onreset={move.overridden ? () => setMove(move.id, null) : undefined}
              ariaLabel="Key for {move.title}"
            />
          </span>
        {/snippet}
      </Row>
    {/each}
  </Section>

  <Section
    label="Action keys"
    description="A key that runs one of the actions on the selected result without opening the action panel first. The panel draws these down its right hand side, so the key is on screen beside the thing it does. Backspace while recording puts one back to what it shipped with; Delete takes the key away."
  >
    <!-- not a setting: a count of the rows below, and a filter over them -->
    <Row
      title="{bound} of {actionKeys.length} actions have a key"
      description="Grouped by what they act on. Type to find one."
    >
      {#snippet control()}
        <div class="controls">
          <TextField
            value={filter}
            oninput={(next) => (filter = next)}
            placeholder="Find an action"
            ariaLabel="Find an action"
          />
          <label class="show">
            <Toggle checked={showUnbound} onchange={(on: boolean) => (showUnbound = on)} label="Show actions with no key" />
            <span>Show those with no key</span>
          </label>
        </div>
      {/snippet}
    </Row>

    {#each shownActions as row, at (row.id)}
      {#if at === 0 || shownActions[at - 1].group !== row.group}
        <div class="group">{row.group}</div>
      {/if}
      <Row title={row.title} description={row.overridden ? "Set by hand" : ""}>
        {#snippet control()}
          <span data-chord={row.chord}>
            <KeyRecorder
              chord={row.chord}
              scope="action"
              section={SECTIONS.acting}
              contested={row.contested}
              onsave={(chord) => setActionKey(row.id, chord)}
              onreset={() => setActionKey(row.id, null)}
              onclear={() => setActionKey(row.id, "")}
              placeholder="No key"
              ariaLabel="Key for {row.title}"
            />
          </span>
        {/snippet}
      </Row>
    {/each}

    {#if shownActions.length === 0}
      <Instead
        tone="empty"
        inline
        headline={filter ? "No action by that name" : "No action has a key"}
        hint={filter ? "Try fewer letters." : "Show those with no key to give one a key."}
      />
    {/if}
  </Section>

  <Section
    label="Window layouts"
    description="Positions of your own, as fractions of the display's work area: left 0, top 0, width 0.5, height 1 is the left half. Each can have a key, which sends the window in front there, and every one is in the action panel on any window."
  >
    {#each layouts as layout, at (layout.id)}
      <Row title={layout.name || "Unnamed layout"} description="Left, top, width, height">
        <div class="controls">
          <input
            class="name"
            value={layout.name}
            placeholder="Name"
            aria-label="Layout name"
            spellcheck="false"
            onchange={(e) => updateLayout(at, { name: e.currentTarget.value.trim() })}
          />
          {#each FRACTIONS as field (field)}
            <input
              class="fraction"
              type="number"
              min="0"
              max="1"
              step="0.05"
              value={layout[field]}
              aria-label={field}
              onchange={(e) => {
                const value = fraction(e.currentTarget.value);
                if (value !== null) updateLayout(at, { [field]: value });
              }}
            />
          {/each}
          <Button label="Set a key" onclick={() => void bindLayout(layout)} />
          <Button label="Remove" tone="danger" onclick={() => removeLayout(at)} />
        </div>
      </Row>
    {/each}

    <Row
      title="Custom layouts"
      description="Halves, thirds and quarters are built in. This is for the rest."
    >
      {#snippet control()}
        <Button label="Add a layout" onclick={addLayout} />
      {/snippet}
    </Row>
  </Section>
</div>

<style>
  .controls {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    flex-wrap: wrap;
  }

  .show {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    font-size: var(--text-meta);
    color: var(--text-2);
    cursor: pointer;
  }

  /* A heading inside the card, for the kind the actions below act on. */
  .group {
    padding: var(--space-3) var(--space-4) var(--space-1);
    font-size: var(--text-label);
    font-weight: var(--weight-strong);
    letter-spacing: var(--track-label);
    text-transform: uppercase;
    color: var(--text-3);
  }

  /* The layout fields: a name and four fractions, sized to what they hold. */
  .name,
  .fraction {
    padding: var(--space-1) var(--space-2);
    font: inherit;
    font-size: var(--text-meta);
    color: var(--text-1);
    background: var(--fill-1);
    border: 0;
    border-radius: var(--radius-sm);
    box-shadow: var(--ring);
  }

  .name {
    inline-size: 12ch;
  }

  .fraction {
    inline-size: 6ch;
    font-variant-numeric: tabular-nums;
  }


  /* The row a key was just added to, so it is found after the scroll. */
  .revealed {
    animation: reveal var(--motion-reading) var(--ease);
  }

  @keyframes reveal {
    from {
      background: var(--accent-fill);
    }
    to {
      background: transparent;
    }
  }
</style>

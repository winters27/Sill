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
  import { actionsFor, searchCommands, type ActionInfo } from "$lib/exthost/commands";
  import type { Binding, BindingSource, Preferences } from "$lib/settings";

  interface Props {
    prefs: Preferences;
    commit: (next: Preferences) => void;
  }

  let { prefs, commit }: Props = $props();

  const bindings = $derived(prefs.bindings ?? []);

  /** Everything that can be done to text, which is what a key can bind to. */
  let textActions = $state<ActionInfo[]>([]);
  /** Titles for the commands bindings point at, so the list reads as names. */
  let commandNames = $state<Record<string, string>>({});

  let recording = $state<number | null>(null);
  let status = $state("");

  onMount(async () => {
    textActions = await actionsFor("text");
  });

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
      description="Runs on {describe(binding.source)}"
    >
      <div class="controls">
        <button
          class="key"
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

        <select
          value={binding.action}
          onchange={(e) => update(at, { action: e.currentTarget.value })}
        >
          {#each textActions as action (action.id)}
            <option value={action.id}>{action.title}</option>
          {/each}
        </select>

        <select
          value={binding.source.from}
          onchange={(e) =>
            update(at, {
              source: e.currentTarget.value === "clipboard"
                ? { from: "clipboard" }
                : { from: "selection" },
            })}
        >
          <option value="selection">Selection</option>
          <option value="clipboard">Clipboard</option>
        </select>

        <Button label="Remove" tone="danger" onclick={() => remove(at)} />
      </div>
    </Row>
  {/each}

  {#if bindings.length === 0}
    <p class="empty">
      No shortcuts yet. One that upper-cases the selection is a good first one.
    </p>
  {/if}

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

<style>
  .controls {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }

  .key {
    min-width: 118px;
    padding: 5px 10px;
    font: inherit;
    font-size: 12px;
    font-variant-numeric: tabular-nums;
    color: var(--text);
    background: var(--surface-raised);
    border: 1px solid var(--line);
    border-radius: 6px;
    cursor: pointer;
  }

  .key.recording {
    color: var(--accent-bright);
    border-color: var(--accent-bright);
  }

  select {
    padding: 5px 8px;
    font: inherit;
    font-size: 12px;
    color: var(--text);
    background: var(--surface-raised);
    border: 1px solid var(--line);
    border-radius: 6px;
  }

  .empty {
    margin: 0;
    padding: 4px 0 8px;
    font-size: 12px;
    color: var(--text-dim);
  }

  .foot {
    display: flex;
    align-items: center;
    gap: 12px;
    padding-top: 8px;
  }

  .status {
    font-size: 12px;
    color: var(--text-dim);
  }
</style>

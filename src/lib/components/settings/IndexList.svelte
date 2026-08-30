<script lang="ts">
  /**
   * Everything in the index, and the three ways to reach it.
   *
   * An alias, a key, and being in the list at all are the same question asked
   * three ways, so they are three columns on one row rather than three
   * unrelated screens. Sill had them scattered: aliases nowhere, hotkeys in
   * the Shortcuts panel keyed by accelerator, and inclusion as a list of words
   * to exclude.
   *
   * The empty cell is the point. A screen that lists only the aliases you
   * already have is one you visit after you know they exist; an empty "Add
   * alias" beside something you were already looking at is the invitation.
   */
  import { onMount } from "svelte";
  import {
    acceleratorFrom,
    indexRows,
    setAlias,
    setCommandHotkey,
    setHidden,
    type IndexRow,
    type Preferences,
  } from "$lib/settings";

  interface Props {
    /** Told whenever a row writes, so the page holds one copy of the truth. */
    onchange: (prefs: Preferences) => void;
  }

  let { onchange }: Props = $props();

  /**
   * The kinds worth separating, in the order a person looks for them.
   *
   * Two modes share one label where the difference is about ranking rather
   * than about what the thing is: an application and a bare executable are
   * both launched identically.
   */
  const KINDS: { id: string; label: string }[] = [
    { id: "all", label: "Everything" },
    { id: "app", label: "Applications" },
    { id: "builtin", label: "Sill" },
    { id: "view", label: "Extensions" },
    { id: "snippet", label: "Snippets" },
    { id: "quicklink", label: "Quicklinks" },
    { id: "setting", label: "Windows settings" },
    { id: "exe", label: "Executables" },
  ];

  let kind = $state("all");
  let query = $state("");
  let rows = $state<IndexRow[]>([]);
  let total = $state(0);
  let loading = $state(true);

  /** The row whose alias is being typed, and what has been typed so far. */
  let naming = $state<string | null>(null);
  let draft = $state("");

  /** The row waiting for a key. */
  let recording = $state<string | null>(null);
  let status = $state("");

  async function refresh() {
    loading = true;
    const page = await indexRows(query, kind);
    rows = page.rows;
    total = page.total;
    loading = false;
  }

  // Narrowed in Rust rather than here: the index is around fifteen hundred
  // entries and sending all of them to filter in the window is the payload
  // mistake that was measured once already.
  $effect(() => {
    query;
    kind;
    void refresh();
  });

  onMount(() => void refresh());

  async function saveAlias(row: IndexRow) {
    naming = null;
    try {
      onchange(await setAlias(row.id, draft));
      await refresh();
    } catch (err) {
      status = `${err}`;
    }
  }

  async function record(event: KeyboardEvent, row: IndexRow) {
    event.preventDefault();
    event.stopPropagation();

    if (event.key === "Escape") {
      recording = null;
      return;
    }

    // Backspace clears. There has to be a way back to no key at all.
    if (event.key === "Backspace") {
      recording = null;
      onchange(await setCommandHotkey(row.id, ""));
      await refresh();
      return;
    }

    const accelerator = acceleratorFrom(event);
    if (!accelerator) return;

    recording = null;
    try {
      onchange(await setCommandHotkey(row.id, accelerator));
      await refresh();
    } catch (err) {
      status = `${err}`;
    }
  }

  async function toggle(row: IndexRow) {
    try {
      onchange(await setHidden(row.id, !row.hidden));
      await refresh();
    } catch (err) {
      status = `${err}`;
    }
  }

  function kindName(mode: string): string {
    return KINDS.find((k) => k.id === mode)?.label ?? mode;
  }
</script>

<div class="controls">
  <div class="chips">
    {#each KINDS as choice (choice.id)}
      <button
        class="chip"
        class:on={kind === choice.id}
        onclick={() => (kind = choice.id)}
      >
        {choice.label}
      </button>
    {/each}
  </div>

  <input
    class="filter"
    type="text"
    bind:value={query}
    placeholder="Filter by name"
    spellcheck="false"
  />
</div>

<div class="head">
  <span>Name</span>
  <span>Alias</span>
  <span>Hotkey</span>
  <span class="middle">On</span>
</div>

<div class="rows">
  {#each rows as row (row.id)}
    <div class="row" class:off={row.hidden}>
      <span class="name" title={row.title}>{row.title}</span>

      {#if naming === row.id}
        <input
          class="cell input"
          bind:value={draft}
          onblur={() => void saveAlias(row)}
          onkeydown={(e) => {
            if (e.key === "Enter") void saveAlias(row);
            if (e.key === "Escape") naming = null;
          }}
          placeholder="short name"
          spellcheck="false"
        />
      {:else}
        <button
          class="cell"
          class:set={!!row.alias}
          onclick={() => {
            naming = row.id;
            draft = row.alias ?? "";
          }}
        >
          {row.alias ?? "Add alias"}
        </button>
      {/if}

      <button
        class="cell"
        class:set={!!row.hotkey}
        class:recording={recording === row.id}
        onclick={() => (recording = recording === row.id ? null : row.id)}
        onkeydown={(e) => recording === row.id && record(e, row)}
      >
        {#if recording === row.id}
          Press a key
        {:else if row.hotkey}
          {row.hotkey.split("+").join(" ")}
        {:else}
          Add hotkey
        {/if}
      </button>

      <button
        class="tick"
        class:on={!row.hidden}
        onclick={() => void toggle(row)}
        aria-label={row.hidden ? `Show ${row.title}` : `Hide ${row.title}`}
      >
        {#if row.hidden}
          <svg width="12" height="12" viewBox="0 0 24 24" aria-hidden="true">
            <path d="M5 12h14" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" />
          </svg>
        {:else}
          <svg width="12" height="12" viewBox="0 0 24 24" aria-hidden="true">
            <path d="M4 12.5 9.5 18 20 6.5" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round" />
          </svg>
        {/if}
      </button>
    </div>
  {/each}
</div>

<p class="foot">
  {#if loading}
    Reading the index…
  {:else if total === 0}
    Nothing matches.
  {:else if total > rows.length}
    <!-- Said out loud. A list that quietly stops at two hundred looks like
         two hundred is all there is. -->
    Showing {rows.length.toLocaleString()} of {total.toLocaleString()}. Filter to narrow it.
  {:else}
    {total.toLocaleString()}
    {total === 1 ? "entry" : "entries"}
    {kind === "all" ? "" : `in ${kindName(kind).toLowerCase()}`}
  {/if}
  {#if status}<span class="status">{status}</span>{/if}
</p>

<style>
  .controls {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    padding-bottom: var(--space-2);
  }

  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1);
  }

  /* No border: a row of bordered pills reads as eight buttons competing for
     attention rather than one control with eight settings. */
  .chip {
    padding: 2px var(--space-2);
    font: inherit;
    font-size: var(--text-label);
    color: var(--text-2);
    background: transparent;
    border: none;
    border-radius: var(--radius-sm);
    cursor: pointer;
  }

  .chip:hover {
    color: var(--text);
  }

  .chip.on {
    color: var(--accent-bright);
    background: var(--fill-2);
  }

  .filter {
    width: 100%;
    padding: var(--space-1) var(--space-2);
    font: inherit;
    font-size: var(--text-meta);
    color: var(--text);
    background: var(--surface-raised);
    border: 1px solid var(--hairline);
    border-radius: var(--radius-md);
  }

  .head,
  .row {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 92px 104px 26px;
    gap: 0 var(--space-2);
    align-items: center;
  }

  .head {
    padding: 0 var(--space-1) var(--space-1);
    font-size: var(--text-micro);
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--text-3);
    border-bottom: 1px solid var(--hairline);
  }

  .middle {
    text-align: center;
  }

  .rows {
    max-height: 420px;
    overflow-y: auto;
  }

  .row {
    padding: var(--space-1) var(--space-1);
    border-bottom: 1px solid var(--hairline);
  }

  .row:last-child {
    border-bottom: none;
  }

  /* Dimmed rather than removed. A row you switched off has to stay findable,
     or switching it back on means remembering it existed. */
  .row.off .name {
    color: var(--text-3);
    text-decoration: line-through;
  }

  .name {
    font-size: var(--text-meta);
    color: var(--text);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .cell {
    padding: 2px var(--space-1);
    font: inherit;
    font-size: var(--text-label);
    text-align: left;
    color: var(--text-3);
    background: transparent;
    border: none;
    border-radius: var(--radius-sm);
    cursor: pointer;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .cell:hover {
    color: var(--text);
    background: var(--surface-raised);
  }

  .cell.set {
    color: var(--accent-bright);
  }

  .cell.recording {
    color: var(--accent-bright);
    background: var(--fill-2);
  }

  .cell.input {
    color: var(--text);
    background: var(--surface-raised);
    cursor: text;
  }

  .tick {
    display: grid;
    place-items: center;
    width: 20px;
    height: 20px;
    padding: 0;
    color: var(--text-3);
    background: transparent;
    border: none;
    border-radius: var(--radius-sm);
    cursor: pointer;
  }

  .tick.on {
    color: var(--accent-bright);
  }

  .foot {
    margin: 0;
    padding-top: var(--space-2);
    font-size: var(--text-label);
    color: var(--text-2);
  }

  .status {
    padding-left: var(--space-2);
    color: var(--danger, #d24b4b);
  }
</style>
